//! T566: `layoutErGraph.ts` port — layered ranks, ordering and coordinates,
//! the grid fallback, the parallel-edge collapse, seeded fuzz and the
//! 500-table budget. Ported from `er/__tests__/layoutErGraph.test.ts`,
//! `layoutErGraph.fallback.test.ts`, `layoutErGraph.parallel.test.ts` and
//! `layoutErGraph.scale.test.ts`.
//!
//! The React suite mocks dagre throwing to reach the fallback; the Rust engine
//! reports refusal as `Err`, so the same contract is pinned by feeding it a
//! shape it refuses (two nodes with one id) and by probing `grid_centres`
//! directly. Constants are asserted as literals, not through the modules' own
//! exports, so a slip in a constant cannot pass its own test.

use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use datazen_ui::views::er::graph::{ErEdge, ErEdgeData, ErNode, ErPosition, ErRelationKind};
use datazen_ui::views::er::layout_er_graph::{
    layout_er_graph, layout_er_graph_default, ErLayoutDirection, ErLayoutOptions,
};
use datazen_ui::views::er::layout_layers::{grid_centres, layered_centres, layout_edges, LayoutEdge};

fn node(id: &str, height: f32) -> ErNode {
    ErNode {
        id: id.to_owned(),
        position: ErPosition::new(0.0, 0.0),
        width: 260.0,
        height,
        ..ErNode::default()
    }
}

fn edge(id: &str, source: &str, target: &str, kind: ErRelationKind) -> ErEdge {
    ErEdge {
        id: id.to_owned(),
        source: source.to_owned(),
        target: target.to_owned(),
        data: ErEdgeData {
            kind,
            ..ErEdgeData::default()
        },
        ..ErEdge::default()
    }
}

fn declared(id: &str, source: &str, target: &str) -> ErEdge {
    edge(id, source, target, ErRelationKind::Declared)
}

fn predicted(id: &str, source: &str, target: &str) -> ErEdge {
    edge(id, source, target, ErRelationKind::Predicted)
}

fn at(nodes: &[ErNode], id: &str) -> ErPosition {
    nodes
        .iter()
        .find(|n| n.id == id)
        .expect("node on canvas")
        .position
}

fn positions_by_id(laid: &[ErNode]) -> BTreeMap<String, ErPosition> {
    laid.iter().map(|n| (n.id.clone(), n.position)).collect()
}

/// Every node placed, finite, and not on top of another node.
fn expect_sane(laid: &[ErNode]) {
    let mut seen: HashSet<String> = HashSet::new();
    for placed in laid {
        assert!(placed.position.x.is_finite(), "{} x", placed.id);
        assert!(placed.position.y.is_finite(), "{} y", placed.id);
        let key = format!("{},{}", placed.position.x, placed.position.y);
        assert!(seen.insert(key.clone()), "{} overlaps {key}", placed.id);
    }
}

// --- layoutErGraph.test.ts -------------------------------------------------

#[test]
fn returns_nothing_for_an_empty_graph() {
    assert!(layout_er_graph(&[], &[], ErLayoutOptions::default()).is_empty());
}

#[test]
fn keeps_the_callers_node_order() {
    // The canvas draws in array order, so the layout must not reorder.
    let nodes = vec![node("c", 100.0), node("a", 100.0), node("b", 100.0)];
    let laid = layout_er_graph_default(&nodes, &[]);
    let ids: Vec<&str> = laid.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b"]);
}

#[test]
fn does_not_mutate_the_nodes_it_is_given() {
    let nodes = vec![node("a", 100.0), node("b", 100.0)];
    let _ = layout_er_graph_default(&nodes, &[declared("e1", "a", "b")]);
    assert_eq!(nodes[0].position, ErPosition::new(0.0, 0.0));
    assert_eq!(nodes[1].position, ErPosition::new(0.0, 0.0));
}

#[test]
fn puts_a_referenced_table_to_the_right_of_the_one_referencing_it() {
    let laid = layout_er_graph_default(
        &[node("child", 100.0), node("parent", 100.0)],
        &[declared("e1", "child", "parent")],
    );
    assert!(at(&laid, "parent").x > at(&laid, "child").x);
}

#[test]
fn runs_a_chain_of_references_in_one_direction() {
    let laid = layout_er_graph_default(
        &[node("a", 100.0), node("b", 100.0), node("c", 100.0)],
        &[declared("e1", "a", "b"), declared("e2", "b", "c")],
    );
    assert!(at(&laid, "b").x > at(&laid, "a").x);
    assert!(at(&laid, "c").x > at(&laid, "b").x);
}

#[test]
fn pads_the_graph_so_no_node_sits_on_the_canvas_edge() {
    let laid = layout_er_graph_default(&[node("a", 100.0)], &[]);
    // The literal margin is the contract the React test pins through
    // ER_LAYOUT_MARGIN; asserting the literal keeps a slip in the constant
    // from passing its own test.
    assert_eq!(at(&laid, "a"), ErPosition::new(24.0, 24.0));
}

#[test]
fn never_overlaps_two_nodes() {
    let nodes = vec![node("a", 338.0), node("b", 62.0), node("c", 146.0)];
    let laid = layout_er_graph_default(&nodes, &[declared("e1", "a", "b"), declared("e2", "b", "c")]);
    for i in 0..laid.len() {
        for j in i + 1..laid.len() {
            let (p, q) = (&laid[i], &laid[j]);
            let overlap_x = p.position.x < q.position.x + q.width
                && q.position.x < p.position.x + p.width;
            let overlap_y = p.position.y < q.position.y + q.height
                && q.position.y < p.position.y + p.height;
            assert!(!(overlap_x && overlap_y), "{} overlaps {}", p.id, q.id);
        }
    }
}

#[test]
fn is_deterministic() {
    let nodes = vec![
        node("a", 100.0),
        node("b", 100.0),
        node("c", 100.0),
        node("d", 100.0),
    ];
    let edges = vec![declared("e1", "a", "b"), declared("e2", "c", "d")];
    let first = layout_er_graph_default(&nodes, &edges);
    let second = layout_er_graph_default(&nodes, &edges);
    assert_eq!(positions_by_id(&second), positions_by_id(&first));
}

#[test]
fn does_not_depend_on_the_order_the_nodes_arrive_in() {
    let edges = vec![declared("e1", "a", "b"), declared("e2", "b", "c")];
    let forward = layout_er_graph_default(
        &[node("a", 100.0), node("b", 100.0), node("c", 100.0)],
        &edges,
    );
    let shuffled = layout_er_graph_default(
        &[node("c", 100.0), node("a", 100.0), node("b", 100.0)],
        &edges,
    );
    assert_eq!(positions_by_id(&shuffled), positions_by_id(&forward));
}

#[test]
fn lays_out_disconnected_tables_rather_than_stacking_them() {
    let laid = layout_er_graph_default(&[node("a", 100.0), node("b", 100.0)], &[]);
    assert_ne!(at(&laid, "a"), at(&laid, "b"));
}

#[test]
fn ignores_an_edge_whose_table_is_not_on_the_canvas() {
    // Focus mode filters tables out; a dangling edge must not make the engine
    // invent a node for a table that is not drawn.
    let laid = layout_er_graph_default(&[node("a", 100.0)], &[declared("e1", "a", "ghost")]);
    let ids: Vec<&str> = laid.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["a"]);
    assert!(at(&laid, "a").x.is_finite());
}

#[test]
fn ranks_a_pair_once_however_many_relationships_they_share() {
    // Two relationships between the same tables are collapsed before the
    // engine sees them; the canvas still draws both.
    let nodes = vec![node("orders", 100.0), node("users", 100.0)];
    let edges = vec![
        declared("e1", "orders", "users"),
        declared("e2", "orders", "users"),
    ];
    assert_eq!(layout_edges(&nodes, &edges).len(), 1);
    let laid = layout_er_graph_default(&nodes, &edges);
    assert_eq!(laid.len(), 2);
    assert!(at(&laid, "users").x > at(&laid, "orders").x);
}

#[test]
fn survives_a_self_referencing_table() {
    let laid = layout_er_graph_default(
        &[node("employee", 100.0)],
        &[declared("e1", "employee", "employee")],
    );
    assert!(at(&laid, "employee").x.is_finite());
    assert!(at(&laid, "employee").y.is_finite());
}

#[test]
fn survives_a_cycle() {
    let laid = layout_er_graph_default(
        &[node("a", 100.0), node("b", 100.0)],
        &[declared("e1", "a", "b"), declared("e2", "b", "a")],
    );
    for n in &laid {
        assert!(n.position.x.is_finite());
        assert!(n.position.y.is_finite());
    }
}

#[test]
fn rounds_positions_to_whole_pixels() {
    let laid = layout_er_graph_default(
        &[node("a", 338.0), node("b", 100.0)],
        &[declared("e1", "a", "b")],
    );
    for n in &laid {
        assert_eq!(n.position.x.fract(), 0.0);
        assert_eq!(n.position.y.fract(), 0.0);
    }
}

#[test]
fn rounds_positions_to_whole_pixels_on_fractional_stacks() {
    // The engine's own arithmetic keeps integer dimensions on whole pixels;
    // a measured height is not an integer, and the stack it creates must
    // still come out on whole pixels — the canvas positions by the corner.
    // Two disconnected tables share a rank, so the second one stacks below
    // the first at a fractional offset.
    let laid = layout_er_graph_default(&[node("a", 100.5), node("b", 62.25)], &[]);
    for n in &laid {
        assert_eq!(n.position.x.fract(), 0.0, "{} x", n.id);
        assert_eq!(n.position.y.fract(), 0.0, "{} y", n.id);
    }
    // The stack really is fractional before the round: 24 + 100.5 + 40.
    assert_eq!(at(&laid, "b"), ErPosition::new(24.0, 165.0));
}

#[test]
fn supports_a_top_to_bottom_direction() {
    let laid = layout_er_graph(
        &[node("child", 100.0), node("parent", 100.0)],
        &[declared("e1", "child", "parent")],
        ErLayoutOptions {
            direction: Some(ErLayoutDirection::TB),
        },
    );
    assert!(at(&laid, "parent").y > at(&laid, "child").y);
}

// --- layoutErGraph.fallback.test.ts -----------------------------------------

#[test]
fn falls_back_to_a_grid_when_the_engine_refuses_the_input() {
    // The Rust engine's refusal surface is its `Err`: two nodes sharing one id
    // would make its centre map key them as one, so it refuses rather than
    // hand back half a ranking. React reaches the same branch by mocking
    // dagre's throw; here the refusal is real.
    let nodes = vec![
        node("a", 110.0),
        node("b", 200.0),
        node("c", 110.0),
        node("d", 110.0),
        node("e", 110.0),
        node("a", 110.0),
    ];
    assert!(layered_centres(&nodes, &[], &ErLayoutOptions::default()).is_err());

    let laid = layout_er_graph_default(&nodes, &[declared("e1", "a", "b")]);

    assert_eq!(laid.len(), nodes.len());
    let mut seen: HashSet<String> = HashSet::new();
    for placed in &laid {
        assert!(placed.position.x.is_finite());
        assert!(placed.position.y.is_finite());
        assert!(placed.position.x >= 0.0);
        assert!(placed.position.y >= 0.0);
        // Distinct ids get one grid cell each; the shared id shares a cell,
        // exactly as React Flow's id-keyed model does.
        if placed.id != "a" {
            let key = format!("{},{}", placed.position.x, placed.position.y);
            assert!(seen.insert(key.clone()), "stacked at {key}");
        }
    }
}

#[test]
fn the_fallback_keeps_the_callers_node_order() {
    let nodes = vec![
        node("c", 100.0),
        node("a", 100.0),
        node("b", 100.0),
        node("a", 100.0),
    ];
    assert!(layered_centres(&nodes, &[], &ErLayoutOptions::default()).is_err());
    let laid = layout_er_graph_default(&nodes, &[]);
    let ids: Vec<&str> = laid.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["c", "a", "b", "a"]);
}

#[test]
fn the_fallback_does_not_mutate_the_nodes_it_is_given() {
    let nodes = vec![node("a", 100.0), node("b", 100.0), node("a", 100.0)];
    let _ = layout_er_graph_default(&nodes, &[declared("e1", "a", "b")]);
    for n in &nodes {
        assert_eq!(n.position, ErPosition::new(0.0, 0.0));
    }
}

#[test]
fn grid_cells_never_overlap_and_anchor_at_the_margin() {
    // The one thing the old grid got wrong: rows must use the tallest node,
    // so a wide table above a narrow one cannot overlap it.
    let nodes = vec![
        node("a", 338.0),
        node("b", 62.0),
        node("c", 146.0),
        node("d", 90.0),
        node("e", 210.0),
    ];
    let centres = grid_centres(&nodes);

    // First cell centre: margin + half the cell (widest 260, tallest 338).
    let a = centres["a"];
    assert_eq!(a, (24.0 + (260.0 + 40.0) / 2.0, 24.0 + (338.0 + 40.0) / 2.0));

    // Corners must not overlap pairwise.
    let corners: BTreeMap<&str, (f32, f32)> = nodes
        .iter()
        .map(|n| {
            let (cx, cy) = centres[n.id.as_str()];
            (n.id.as_str(), (cx - n.width / 2.0, cy - n.height / 2.0))
        })
        .collect();
    let dims: BTreeMap<&str, (f32, f32)> = nodes
        .iter()
        .map(|n| (n.id.as_str(), (n.width, n.height)))
        .collect();
    let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            let (p, q) = (corners[ids[i]], corners[ids[j]]);
            let (pw, ph) = dims[ids[i]];
            let (qw, qh) = dims[ids[j]];
            let overlap_x = p.0 < q.0 + qw && q.0 < p.0 + pw;
            let overlap_y = p.1 < q.1 + qh && q.1 < p.1 + ph;
            assert!(!(overlap_x && overlap_y), "{} overlaps {}", ids[i], ids[j]);
        }
    }
}

// --- layoutErGraph.parallel.test.ts -----------------------------------------

#[test]
fn lays_out_the_shape_that_used_to_throw() {
    // Three edges t2→t0 (rank 2 → 4 in dagre, so each needed a dummy chain)
    // plus a cycle t1↔t2: the minimal reproduction of the dagre failure that
    // used to blank the diagram. These shapes must lay out on the layered
    // path — the grid is not what saved us.
    let nodes = vec![node("t0", 110.0), node("t1", 110.0), node("t2", 110.0)];
    let edges = vec![
        predicted("p0", "t2", "t0"),
        predicted("p1", "t2", "t0"),
        predicted("p2", "t1", "t2"),
        predicted("p3", "t1", "t0"),
        predicted("p4", "t2", "t0"),
        predicted("p5", "t2", "t1"),
    ];
    assert!(layered_centres(&nodes, &edges, &ErLayoutOptions::default()).is_ok());
    expect_sane(&layout_er_graph_default(&nodes, &edges));
}

#[test]
fn a_declared_constraint_pulls_harder_than_an_inference() {
    // Collapsing parallel edges keeps the strongest weight, so this is the
    // same diagram whether the second relationship is declared or inferred —
    // and whichever order the two arrive in.
    let nodes = vec![node("orders", 110.0), node("users", 110.0)];
    let inferred_then_declared = vec![
        predicted("e1", "orders", "users"),
        declared("e2", "orders", "users"),
    ];
    let both_declared = vec![
        declared("e1", "orders", "users"),
        declared("e2", "orders", "users"),
    ];
    assert_eq!(
        layout_edges(&nodes, &inferred_then_declared),
        vec![LayoutEdge {
            source: "orders".to_owned(),
            target: "users".to_owned(),
            weight: 2.0,
        }]
    );
    let laid_inferred = layout_er_graph_default(&nodes, &inferred_then_declared);
    let laid_declared = layout_er_graph_default(&nodes, &both_declared);
    expect_sane(&laid_inferred);
    assert_eq!(positions_by_id(&laid_inferred), positions_by_id(&laid_declared));
    assert!(at(&laid_inferred, "users").x > at(&laid_inferred, "orders").x);
}

#[test]
fn survives_parallel_edges_with_a_self_loop_and_a_cycle_together() {
    let nodes = vec![node("a", 110.0), node("b", 110.0), node("c", 110.0)];
    let edges = vec![
        predicted("e1", "a", "b"),
        predicted("e2", "a", "b"),
        predicted("e3", "b", "a"),
        predicted("e4", "b", "b"),
        predicted("e5", "a", "c"),
        predicted("e6", "c", "a"),
    ];
    assert!(layered_centres(&nodes, &edges, &ErLayoutOptions::default()).is_ok());
    expect_sane(&layout_er_graph_default(&nodes, &edges));
}

#[test]
fn lays_out_several_hundred_randomly_shaped_graphs() {
    // The dagre failure needed a specific combination, so a handful of
    // hand-written shapes is not enough cover. The generator is the React
    // suite's, ported bit-for-bit, so a regression is reproducible from the
    // seed in the failure message.
    let options = ErLayoutOptions::default();
    for seed in 1u32..=400 {
        let mut state = seed;
        let random = |state: &mut u32| -> f64 {
            *state = state.wrapping_add(0x6d2b79f5);
            let mut t = (*state ^ (*state >> 15)).wrapping_mul(1 | *state);
            t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
            f64::from(t ^ (t >> 14)) / 4294967296.0
        };

        let table_count = 2 + (random(&mut state) * 8.0) as usize;
        let nodes: Vec<ErNode> = (0..table_count)
            .map(|index| {
                let height = 60.0 + ((random(&mut state) * 8.0) as usize) as f32 * 40.0;
                node(&format!("t{index}"), height)
            })
            .collect();
        let edge_count = (random(&mut state) * 12.0) as usize;
        let edges: Vec<ErEdge> = (0..edge_count)
            .map(|index| {
                let source = format!("t{}", (random(&mut state) * table_count as f64) as usize);
                let target = format!("t{}", (random(&mut state) * table_count as f64) as usize);
                predicted(&format!("e{index}"), &source, &target)
            })
            .collect();

        // The layered path must handle every shape, not fall back.
        assert!(
            layered_centres(&nodes, &edges, &options).is_ok(),
            "seed {seed}"
        );
        let laid = layout_er_graph_default(&nodes, &edges);
        assert_eq!(laid.len(), table_count, "seed {seed}");
        expect_sane(&laid);
    }
}

// --- layoutErGraph.scale.test.ts --------------------------------------------

#[test]
fn lays_out_a_500_table_schema_inside_an_interactive_budget() {
    // A database can hold hundreds of tables and the diagram lays out all of
    // them on every schema load. The bound is far above the real cost so CI
    // jitter cannot fail it, while a layout that became accidentally
    // quadratic still would. React builds the schema through buildErGraph;
    // that builder is T562 and not ported yet, so the nodes are constructed
    // directly — the cost under test is the layout's.
    let nodes: Vec<ErNode> = (0..500)
        .map(|t| node(&format!("entity_{t}"), 240.0))
        .collect();
    let edges: Vec<ErEdge> = (1..500)
        .filter(|t| t % 3 == 0)
        .map(|t| declared(&format!("fk_{t}"), &format!("entity_{t}"), &format!("entity_{}", t - 1)))
        .collect();

    let started = Instant::now();
    let laid = layout_er_graph_default(&nodes, &edges);
    let elapsed = started.elapsed().as_millis();

    assert_eq!(laid.len(), 500);
    // A fast wrong answer is no answer: every node must have a real position.
    for n in &laid {
        assert!(n.position.x.is_finite());
        assert!(n.position.y.is_finite());
    }
    assert!(elapsed < 3000, "500-table layout took {elapsed}ms");
}

// --- engine front door -------------------------------------------------------

#[test]
fn layout_edges_drop_unknown_endpoints_self_loops_and_sort_canonically() {
    let nodes = vec![node("b", 100.0), node("a", 100.0), node("c", 100.0)];
    let edges = vec![
        predicted("e1", "b", "a"),
        // Predicted first, declared second: the collapse must keep the
        // strongest kind whichever order they arrive in.
        predicted("e2", "a", "b"),
        declared("e3", "a", "b"),
        declared("e4", "a", "ghost"),
        declared("e5", "c", "c"),
    ];
    assert_eq!(
        layout_edges(&nodes, &edges),
        vec![
            LayoutEdge {
                source: "a".to_owned(),
                target: "b".to_owned(),
                weight: 2.0,
            },
            LayoutEdge {
                source: "b".to_owned(),
                target: "a".to_owned(),
                weight: 1.0,
            },
        ]
    );
}
