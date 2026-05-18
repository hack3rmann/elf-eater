use petgraph::{
    Direction,
    algo::dominators::Dominators,
    graph::{DiGraph, NodeIndex},
    visit::IntoNodeIdentifiers,
};
use smallvec::{SmallVec, smallvec};

pub fn dominance_frontiers<N, E>(
    graph: &DiGraph<N, E>,
    doms: &Dominators<NodeIndex>,
) -> Vec<SmallVec<[NodeIndex; 4]>> {
    let mut df = vec![smallvec![]; graph.node_count()];

    for node in graph.node_identifiers() {
        // collect predecessors
        let preds = graph
            .neighbors_directed(node, Direction::Incoming)
            .collect::<Vec<_>>();

        // only join points matter
        if preds.len() < 2 {
            continue;
        }

        for pred in preds {
            let mut runner = pred;

            while Some(runner) != doms.immediate_dominator(node) {
                df[runner.index()].push(node);

                match doms.immediate_dominator(runner) {
                    Some(idom) => runner = idom,
                    None => break,
                }
            }
        }
    }

    df
}
