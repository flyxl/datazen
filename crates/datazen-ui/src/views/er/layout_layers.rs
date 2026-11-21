//! The layered-layout engine for the ER diagram (T566 split).
//!
//! React lays the diagram out with dagre (Sugiyama-style). Rust has no dagre
//! here, and pulling one in would be the first native dependency of the UI
//! crate, so this module implements the same layered contract directly:
//!
//! 1. **Canonical order** — nodes and the collapsed edge list are processed
//!    sorted by id, so the same schema always produces the same diagram and
//!    the result never depends on the order the tables arrived in (the React
//!    source inserts sorted for exactly this reason).
//! 2. **Cycle breaking** — a depth-first pass drops the back edges, which are
//!    the only edges that can make ranking non-convergent. A self-loop is
//!    dropped earlier, when parallel edges are collapsed.
//! 3. **Ranking** — a topological longest-path pass over the remaining
//!    acyclic edges gives every referenced table a strictly higher rank than
//!    the table referencing it (`child → parent`, parents to the right in
//!    `LR`). Longest-path is what keeps a relationship on a single rank hop
//!    in the shapes the React tests pin.
//! 4. **Ordering** — barycenter sweeps minimise crossings by construction,
//!    weighted so a declared constraint pulls its tables together harder
//!    than an inference does (dagre's edge weights). Isolated nodes keep
//!    their slot, so sweeps never shuffle unrelated tables.
//! 5. **Coordinates** — each rank is a strip on the rank axis sized by its
//!    widest node plus [`RANK_SEP`]; within a rank nodes stack on the order
//!    axis with [`NODE_SEP`] between actual heights, so a tall table never
//!    overlaps the node below it. The margin pins the first strip and the
//!    first stack, so nothing sits on the canvas edge.
//!
//! The engine reports centres, like dagre does; the caller converts to
//! top-left corners. A validation pass keeps the React fallback honest: the
//! ranking reports failure if the topological pass ever comes up short (the
//! cycle breaking above makes that unreachable today, but the caller must
//! never hand back half-ranked coordinates), and a non-finite centre fails
//! the same way. On failure the caller degrades to [`grid_centres`], exactly
//! as the React source degrades when dagre throws.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::views::er::graph::{ErEdge, ErNode, ErRelationKind};
use crate::views::er::layout_er_graph::{
    ErLayoutDirection, ErLayoutOptions, ER_LAYOUT_MARGIN, NODE_SEP, RANK_SEP,
};

/// A declared constraint should shape the diagram more than an inference does.
pub(crate) const DECLARED_WEIGHT: f32 = 2.0;
const PREDICTED_WEIGHT: f32 = 1.0;

/// One edge per ordered pair of tables — all the engine needs to rank and
/// order them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LayoutEdge {
    pub source: String,
    pub target: String,
    pub weight: f32,
}

/// Read the layout weight of an edge from the kind the graph builder tagged
/// it with.
fn edge_weight(edge: &ErEdge) -> f32 {
    match edge.data.kind {
        ErRelationKind::Predicted => PREDICTED_WEIGHT,
        ErRelationKind::Declared => DECLARED_WEIGHT,
    }
}

/// Reduce the caller's edges to the ones that can shape the layout: one per
/// ordered pair of tables, self-loops dropped, unknown endpoints dropped.
///
/// Parallel edges are exactly what prediction produces (a declared constraint
/// plus an inferred relation on the same pair, or two inferences between the
/// same tables), and dagre 3.1.1 loses a dummy node on that shape — the React
/// source collapses them before dagre sees them, so the Rust engine receives
/// the same collapsed list. The strongest kind wins, so a declared constraint
/// still pulls its tables together harder than an inference does.
///
/// The result is sorted by `(source, target)`: the engine's own bookkeeping
/// depends on insertion order, and a canonical order is what makes the layout
/// deterministic and arrival-order independent.
pub(crate) fn layout_edges(nodes: &[ErNode], edges: &[ErEdge]) -> Vec<LayoutEdge> {
    let known: HashSet<&str> = nodes.iter().map(|node| node.id.as_str()).collect();
    let mut by_pair: BTreeMap<(&str, &str), f32> = BTreeMap::new();
    for edge in edges {
        // An edge to a table that is not on the canvas (filtered out by focus
        // mode) would otherwise make the engine invent a node for it.
        if !known.contains(edge.source.as_str()) || !known.contains(edge.target.as_str()) {
            continue;
        }
        // A self-loop cannot move its own node, and the label space a layout
        // engine reserves for it is space the canvas never draws into.
        if edge.source == edge.target {
            continue;
        }
        let weight = edge_weight(edge);
        let entry = by_pair
            .entry((edge.source.as_str(), edge.target.as_str()))
            .or_insert(weight);
        if *entry < weight {
            *entry = weight;
        }
    }
    by_pair
        .into_iter()
        .map(|((source, target), weight)| LayoutEdge {
            source: source.to_owned(),
            target: target.to_owned(),
            weight,
        })
        .collect()
}

/// The layered engine's answer, or the reason it refused to give one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LayeredFailure {
    /// The topological pass fell short or a centre came out non-finite; the
    /// caller must fall back to the grid rather than draw half a ranking.
    Unranks,
}

/// Centres produced by the layered engine, keyed by node id. The React source
/// throws when dagre cannot order the input; here the same contract is a
/// `Result`, and the caller degrades exactly as React does.
pub(crate) fn layered_centres(
    nodes: &[ErNode],
    edges: &[ErEdge],
    options: &ErLayoutOptions,
) -> Result<HashMap<String, (f32, f32)>, LayeredFailure> {
    let direction = options.direction.unwrap_or(ErLayoutDirection::LR);
    // The engine works on indices over the id-sorted node list — the same
    // canonical order the React source inserts into dagre with. `ids` is
    // derived from `ordered` (not re-sorted) so index `i` always means the
    // same node in both, whatever order the caller handed them over in.
    let mut ordered: Vec<&ErNode> = nodes.iter().collect();
    ordered.sort_unstable_by(|a, b| a.id.cmp(&b.id));
    let ids: Vec<&str> = ordered.iter().map(|node| node.id.as_str()).collect();
    let index_of: HashMap<&str, usize> =
        ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    let collapsed = layout_edges(nodes, edges);
    let n = ids.len();

    // Directed adjacency over indices (child → parent), targets in sorted
    // order so the DFS below breaks cycles deterministically.
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in &collapsed {
        let (s, t) = (index_of[edge.source.as_str()], index_of[edge.target.as_str()]);
        adjacency[s].push(t);
    }
    for list in &mut adjacency {
        list.sort_unstable();
    }

    // --- Cycle breaking: DFS from the sorted order, dropping back edges. ---
    // 0 = unvisited, 1 = on the stack, 2 = done.
    let mut colour = vec![0u8; n];
    let mut broken: HashSet<(usize, usize)> = HashSet::new();
    fn dfs(node: usize, adjacency: &[Vec<usize>], colour: &mut [u8], broken: &mut HashSet<(usize, usize)>) {
        colour[node] = 1;
        for &next in &adjacency[node] {
            match colour[next] {
                0 => dfs(next, adjacency, colour, broken),
                1 => {
                    broken.insert((node, next));
                }
                _ => {}
            }
        }
        colour[node] = 2;
    }
    for start in 0..n {
        if colour[start] == 0 {
            dfs(start, &adjacency, &mut colour, &mut broken);
        }
    }

    // --- Ranking: longest path over the acyclic remainder. ---
    let mut rank = vec![0i32; n];
    let mut forward: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
    let mut reverse: Vec<Vec<(usize, f32)>> = vec![Vec::new(); n];
    let mut indegree = vec![0usize; n];
    for edge in &collapsed {
        let (s, t) = (index_of[edge.source.as_str()], index_of[edge.target.as_str()]);
        if broken.contains(&(s, t)) {
            continue;
        }
        forward[s].push((t, edge.weight));
        reverse[t].push((s, edge.weight));
        indegree[t] += 1;
    }
    // Kahn's queue, seeded and popped in index order for determinism.
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut processed = 0usize;
    while let Some(node) = queue.pop_front() {
        processed += 1;
        for &(next, _weight) in &forward[node] {
            if rank[next] < rank[node] + 1 {
                rank[next] = rank[node] + 1;
            }
            indegree[next] -= 1;
            if indegree[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    // The cycle breaking above guarantees a DAG, so every node is processed.
    // If a future edit breaks that invariant, the layout must fall back
    // rather than hand back half-ranked coordinates.
    if processed != n {
        return Err(LayeredFailure::Unranks);
    }

    // --- Ordering: barycenter sweeps over the order axis. ---
    let rank_count = rank.iter().copied().max().unwrap_or(0) as usize + 1;
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); rank_count];
    for i in 0..n {
        members[rank[i] as usize].push(i);
    }
    for list in &mut members {
        list.sort_unstable(); // initial within-rank order is id order
    }
    fn order_index(members: &[Vec<usize>], rank: &[i32], node: usize) -> f32 {
        members[rank[node] as usize]
            .iter()
            .position(|&m| m == node)
            .map(|p| p as f32)
            .unwrap_or(0.0)
    }
    let mut barycenter = vec![0.0f32; n];
    for sweep in 0..4 {
        // Alternate sweep direction: ranks ascending, then descending, twice.
        let mut visit: Vec<usize> = (0..rank_count).collect();
        if sweep % 2 == 1 {
            visit.reverse();
        }
        for r in visit {
            // Barycenters are computed against one snapshot of the ordering,
            // then the rank is sorted by them.
            let snapshot = members.clone();
            for &node in &snapshot[r] {
                let mut sum = 0.0f32;
                let mut count = 0.0f32;
                for &(next, weight) in &forward[node] {
                    if rank[next] != r as i32 {
                        sum += order_index(&snapshot, &rank, next) * weight;
                        count += weight;
                    }
                }
                for &(prev, weight) in &reverse[node] {
                    if rank[prev] != r as i32 {
                        sum += order_index(&snapshot, &rank, prev) * weight;
                        count += weight;
                    }
                }
                barycenter[node] = if count > 0.0 {
                    sum / count
                } else {
                    order_index(&snapshot, &rank, node)
                };
            }
            let mut list = members[r].clone();
            list.sort_by(|&a, &b| {
                barycenter[a]
                    .partial_cmp(&barycenter[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.cmp(&b))
            });
            members[r] = list;
        }
    }

    // --- Coordinates: strips on the rank axis, stacks on the order axis. ---
    let horizontal = matches!(direction, ErLayoutDirection::LR);
    let mut centres: HashMap<String, (f32, f32)> = HashMap::with_capacity(n);
    let mut rank_offset = ER_LAYOUT_MARGIN;
    for r in 0..rank_count {
        let members_now = &members[r];
        let rank_span: f32 = members_now
            .iter()
            .map(|&i| {
                if horizontal {
                    ordered[i].width
                } else {
                    ordered[i].height
                }
            })
            .fold(0.0f32, f32::max);
        let mut stack_offset = ER_LAYOUT_MARGIN;
        for &i in members_now {
            let (w, h) = (ordered[i].width, ordered[i].height);
            let centre = if horizontal {
                (rank_offset + w / 2.0, stack_offset + h / 2.0)
            } else {
                (stack_offset + w / 2.0, rank_offset + h / 2.0)
            };
            if !centre.0.is_finite() || !centre.1.is_finite() {
                return Err(LayeredFailure::Unranks);
            }
            centres.insert(ids[i].to_owned(), centre);
            stack_offset += if horizontal { h } else { w } + NODE_SEP;
        }
        rank_offset += rank_span + RANK_SEP;
    }

    // Every node the caller handed over must have been placed: leaving one
    // out is what would let a NaN reach the canvas in the React version.
    if centres.len() != nodes.len() {
        return Err(LayeredFailure::Unranks);
    }
    Ok(centres)
}

/// A deterministic grid, used only when the layered engine cannot lay the
/// graph out at all.
///
/// Deliberately dumb: the point is that the ER view still shows every table,
/// with its edges, when the layout fails. Rows use the tallest node so
/// nothing overlaps, which is the one thing the old grid got wrong. Centres,
/// like the layered engine's — the caller converts to corners.
pub(crate) fn grid_centres(nodes: &[ErNode]) -> HashMap<String, (f32, f32)> {
    let mut widest = 0.0f32;
    let mut tallest = 0.0f32;
    for node in nodes {
        widest = widest.max(node.width);
        tallest = tallest.max(node.height);
    }
    let cell_width = widest + NODE_SEP;
    let cell_height = tallest + NODE_SEP;
    let columns = ((nodes.len() as f64).sqrt().ceil() as usize).max(1);

    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let column = index % columns;
            let row = index / columns;
            (
                node.id.clone(),
                (
                    ER_LAYOUT_MARGIN + column as f32 * cell_width + cell_width / 2.0,
                    ER_LAYOUT_MARGIN + row as f32 * cell_height + cell_height / 2.0,
                ),
            )
        })
        .collect()
}
