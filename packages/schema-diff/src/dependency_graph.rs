//! Validated deterministic DAG primitives shared by schema migration planners.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyGraphError<N> {
    DuplicateIdentity(N),
    AmbiguousReference(String),
    MissingIdentity { prerequisite: N, dependent: N },
    SelfDependency(N),
    Cycle(Vec<N>),
    MissingRollback(Vec<N>),
    InvalidApplyOrder,
}

impl<N: Debug> std::fmt::Display for DependencyGraphError<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateIdentity(node) => {
                write!(f, "ambiguous migration identity: {node:?}")
            }
            Self::AmbiguousReference(reference) => {
                write!(f, "ambiguous migration dependency reference: {reference}")
            }
            Self::MissingIdentity {
                prerequisite,
                dependent,
            } => write!(
                f,
                "unresolved migration dependency {prerequisite:?} required by {dependent:?}"
            ),
            Self::SelfDependency(node) => write!(f, "migration dependency cycle at {node:?}"),
            Self::Cycle(nodes) => write!(f, "migration dependency cycle among {nodes:?}"),
            Self::MissingRollback(nodes) => {
                write!(f, "rollback is unavailable for migration nodes {nodes:?}")
            }
            Self::InvalidApplyOrder => write!(f, "apply order does not match the dependency graph"),
        }
    }
}

/// A directed edge means that `prerequisite` must finish before `dependent`.
#[derive(Debug, Clone)]
pub struct DependencyGraph<N: Ord> {
    nodes: BTreeSet<N>,
    dependents: BTreeMap<N, BTreeSet<N>>,
}

impl<N: Ord + Clone> DependencyGraph<N> {
    pub fn new() -> Self {
        Self {
            nodes: BTreeSet::new(),
            dependents: BTreeMap::new(),
        }
    }

    pub fn add_node(&mut self, identity: N) -> Result<(), DependencyGraphError<N>> {
        if !self.nodes.insert(identity.clone()) {
            return Err(DependencyGraphError::DuplicateIdentity(identity));
        }
        self.dependents.entry(identity).or_default();
        Ok(())
    }

    pub fn add_dependency(
        &mut self,
        prerequisite: &N,
        dependent: &N,
    ) -> Result<(), DependencyGraphError<N>> {
        if prerequisite == dependent {
            return Err(DependencyGraphError::SelfDependency(prerequisite.clone()));
        }
        if !self.nodes.contains(prerequisite) || !self.nodes.contains(dependent) {
            return Err(DependencyGraphError::MissingIdentity {
                prerequisite: prerequisite.clone(),
                dependent: dependent.clone(),
            });
        }
        self.dependents
            .entry(prerequisite.clone())
            .or_default()
            .insert(dependent.clone());
        Ok(())
    }

    /// Kahn's algorithm with an ordered ready set makes independent nodes stable.
    pub fn topological_order(&self) -> Result<Vec<N>, DependencyGraphError<N>> {
        let mut indegree = self
            .nodes
            .iter()
            .cloned()
            .map(|node| (node, 0usize))
            .collect::<BTreeMap<_, _>>();
        for dependents in self.dependents.values() {
            for dependent in dependents {
                if let Some(degree) = indegree.get_mut(dependent) {
                    *degree += 1;
                }
            }
        }

        let mut ready = indegree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(node, _)| node.clone())
            .collect::<BTreeSet<_>>();
        let mut ordered = Vec::with_capacity(self.nodes.len());

        while let Some(node) = ready.pop_first() {
            ordered.push(node.clone());
            if let Some(dependents) = self.dependents.get(&node) {
                for dependent in dependents {
                    let Some(degree) = indegree.get_mut(dependent) else {
                        continue;
                    };
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(dependent.clone());
                    }
                }
            }
        }

        if ordered.len() != self.nodes.len() {
            let cycle = indegree
                .into_iter()
                .filter_map(|(node, degree)| (degree > 0).then_some(node))
                .collect();
            return Err(DependencyGraphError::Cycle(cycle));
        }
        Ok(ordered)
    }

    /// Rollback follows the inverse topological order only when every apply
    /// operation has rollback SQL supplied by its renderer.
    pub fn rollback_order(
        &self,
        apply_order: &[N],
        rollback_available: &BTreeSet<N>,
    ) -> Result<Vec<N>, DependencyGraphError<N>> {
        if apply_order.len() != self.nodes.len()
            || apply_order.iter().cloned().collect::<BTreeSet<_>>() != self.nodes
            || self.topological_order()? != apply_order
        {
            return Err(DependencyGraphError::InvalidApplyOrder);
        }
        let missing = apply_order
            .iter()
            .filter(|node| !rollback_available.contains(*node))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(DependencyGraphError::MissingRollback(missing));
        }
        Ok(apply_order.iter().rev().cloned().collect())
    }
}

impl<N: Ord + Clone> Default for DependencyGraph<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_typed_dependency_chain_and_stabilizes_independent_nodes() {
        let chain = [
            "type:status",
            "table:public.orders",
            "fk:orders_user",
            "view:v",
        ];
        let mut graph = DependencyGraph::new();
        for node in chain
            .iter()
            .copied()
            .chain(["table:public.users", "routine:f"])
        {
            graph.add_node(node).unwrap();
        }
        graph.add_dependency(&chain[0], &chain[1]).unwrap();
        graph.add_dependency(&chain[1], &chain[2]).unwrap();
        graph.add_dependency(&chain[2], &chain[3]).unwrap();

        let order = graph.topological_order().unwrap();
        assert!(
            order.iter().position(|node| *node == chain[0]).unwrap()
                < order.iter().position(|node| *node == chain[1]).unwrap()
        );
        assert!(
            order.iter().position(|node| *node == chain[1]).unwrap()
                < order.iter().position(|node| *node == chain[2]).unwrap()
        );
        assert!(
            order.iter().position(|node| *node == chain[2]).unwrap()
                < order.iter().position(|node| *node == chain[3]).unwrap()
        );
        assert_eq!(order, graph.topological_order().unwrap());
    }

    #[test]
    fn rejects_duplicate_missing_self_and_cross_cycle_dependencies() {
        let mut graph = DependencyGraph::new();
        graph.add_node("view:a").unwrap();
        assert_eq!(
            graph.add_node("view:a"),
            Err(DependencyGraphError::DuplicateIdentity("view:a"))
        );
        assert_eq!(
            graph.add_dependency(&"view:a", &"view:missing"),
            Err(DependencyGraphError::MissingIdentity {
                prerequisite: "view:a",
                dependent: "view:missing",
            })
        );
        assert_eq!(
            graph.add_dependency(&"view:a", &"view:a"),
            Err(DependencyGraphError::SelfDependency("view:a"))
        );
        graph.add_node("view:b").unwrap();
        graph.add_dependency(&"view:a", &"view:b").unwrap();
        graph.add_dependency(&"view:b", &"view:a").unwrap();
        assert!(matches!(
            graph.topological_order(),
            Err(DependencyGraphError::Cycle(nodes)) if nodes == vec!["view:a", "view:b"]
        ));
    }

    #[test]
    fn rollback_is_the_reverse_apply_order_only_with_complete_renderer_output() {
        let mut graph = DependencyGraph::new();
        for node in ["table:a", "view:b"] {
            graph.add_node(node).unwrap();
        }
        graph.add_dependency(&"table:a", &"view:b").unwrap();
        let apply = graph.topological_order().unwrap();
        let complete = ["table:a", "view:b"].into_iter().collect();
        assert_eq!(
            graph.rollback_order(&apply, &complete).unwrap(),
            vec!["view:b", "table:a"]
        );
        let incomplete = ["table:a"].into_iter().collect();
        assert_eq!(
            graph.rollback_order(&apply, &incomplete),
            Err(DependencyGraphError::MissingRollback(vec!["view:b"]))
        );
        assert_eq!(
            graph.rollback_order(&["view:b", "table:a"], &complete),
            Err(DependencyGraphError::InvalidApplyOrder)
        );
    }
}
