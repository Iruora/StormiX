//! Which steps a step runs after: the flow as a graph of branches.
//!
//! A step with no `after:` runs after the step written just above it, so a
//! flow that never says `after:` is the straight line it always was. Steps
//! that name the same step run at the same time, each starting a branch; a
//! step that names several steps waits for all of them (a join). A step may
//! only name steps above it, so file order is always a valid run order.

use std::collections::BTreeSet;

use crate::{Flow, Step};

/// The enabled steps of a flow and what each waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepGraph {
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Where the step is in `flow.steps`.
    pub step: usize,
    /// The nodes (positions in [`StepGraph::nodes`]) this one waits for,
    /// in the order the step names them. Turned-off steps in between are
    /// looked through: a step after one waits for what that one waited for.
    pub after: Vec<usize>,
    /// Every node this one runs after, directly or further up.
    pub ancestors: BTreeSet<usize>,
}

impl Flow {
    /// Whether any step says `after:`. Without it, steps run one after the
    /// other in file order.
    pub fn has_branches(&self) -> bool {
        self.steps.iter().any(|s| s.after.is_some())
    }

    /// The steps `flow.steps[index]` names or implies in `after:`, as
    /// indexes into `flow.steps`, enabled or not. Names that do not match a
    /// step above it are left out; [`crate::TestFile::validate`] reports them.
    pub fn declared_after(&self, index: usize) -> Vec<usize> {
        let step = &self.steps[index];
        match &step.after {
            None => index.checked_sub(1).into_iter().collect(),
            Some(names) => {
                let mut out = Vec::new();
                for name in names {
                    // The nearest step above with that name.
                    if let Some(i) = self.steps[..index].iter().rposition(|s| &s.name == name) {
                        if !out.contains(&i) {
                            out.push(i);
                        }
                    }
                }
                out
            }
        }
    }

    /// The enabled steps as a graph, in file order.
    pub fn graph(&self) -> StepGraph {
        // For each step, enabled or not, the enabled steps it waits for.
        let mut effective: Vec<Vec<usize>> = Vec::with_capacity(self.steps.len());
        for i in 0..self.steps.len() {
            let mut deps = Vec::new();
            for d in self.declared_after(i) {
                if self.steps[d].enabled {
                    push_unique(&mut deps, d);
                } else {
                    for &e in &effective[d] {
                        push_unique(&mut deps, e);
                    }
                }
            }
            effective.push(deps);
        }
        let position: Vec<Option<usize>> = {
            let mut next = 0;
            self.steps
                .iter()
                .map(|s| {
                    s.enabled.then(|| {
                        next += 1;
                        next - 1
                    })
                })
                .collect()
        };
        let mut nodes: Vec<Node> = Vec::new();
        for (i, step) in self.steps.iter().enumerate() {
            if !step.enabled {
                continue;
            }
            let after: Vec<usize> = effective[i]
                .iter()
                .filter_map(|&d| position[d])
                .collect();
            let mut ancestors = BTreeSet::new();
            for &a in &after {
                ancestors.insert(a);
                ancestors.extend(nodes[a].ancestors.iter().copied());
            }
            nodes.push(Node {
                step: i,
                after,
                ancestors,
            });
        }
        StepGraph { nodes }
    }
}

impl StepGraph {
    /// Each node waits for the one before it and nothing else.
    pub fn is_linear(&self) -> bool {
        self.nodes
            .iter()
            .enumerate()
            .all(|(i, n)| n.after == i.checked_sub(1).into_iter().collect::<Vec<_>>())
    }

    /// Whether two nodes may run at the same time: neither runs after the other.
    pub fn parallel(&self, a: usize, b: usize) -> bool {
        a != b && !self.nodes[a].ancestors.contains(&b) && !self.nodes[b].ancestors.contains(&a)
    }

    /// The nodes that wait for `node`, directly.
    pub fn next(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .filter(move |(_, n)| n.after.contains(&node))
            .map(|(i, _)| i)
    }
}

fn push_unique(list: &mut Vec<usize>, value: usize) {
    if !list.contains(&value) {
        list.push(value);
    }
}

impl Step {
    /// Whether `after:` names `name`.
    pub fn runs_after(&self, name: &str) -> bool {
        self.after
            .as_ref()
            .is_some_and(|names| names.iter().any(|n| n == name))
    }
}

#[cfg(test)]
mod tests {
    use crate::TestFile;

    fn flow(steps: &str) -> crate::Flow {
        let yaml = format!("version: 1\nflow:\n  name: f\n  steps:\n{steps}");
        TestFile::from_yaml(&yaml).expect("parses").flow
    }

    fn step(name: &str, extra: &str) -> String {
        format!("    - name: {name}\n      request: {{ method: GET, url: http://x/{name} }}\n{extra}")
    }

    #[test]
    fn without_after_is_a_line() {
        let f = flow(&(step("a", "") + &step("b", "") + &step("c", "")));
        let g = f.graph();
        assert!(g.is_linear());
        assert!(!f.has_branches());
        assert_eq!(g.nodes[2].after, vec![1]);
    }

    #[test]
    fn fork_and_join() {
        let f = flow(
            &(step("login", "")
                + &step("p", "      after: [login]\n")
                + &step("o", "      after: [login]\n")
                + &step("last", "")
                + &step("dash", "      after: [p, last]\n")
                + &step("start", "      after: []\n")),
        );
        let g = f.graph();
        assert!(!g.is_linear());
        assert_eq!(g.nodes[1].after, vec![0]);
        assert_eq!(g.nodes[2].after, vec![0]);
        assert_eq!(g.nodes[3].after, vec![2]);
        assert_eq!(g.nodes[4].after, vec![1, 3]);
        assert!(g.nodes[5].after.is_empty());
        assert!(g.parallel(1, 3));
        assert!(!g.parallel(0, 4));
        assert_eq!(g.next(0).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn turned_off_steps_are_looked_through() {
        let f = flow(
            &(step("a", "")
                + &step("b", "      after: [a]\n      enabled: false\n")
                + &step("c", "      after: [b]\n")),
        );
        let g = f.graph();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.nodes[1].after, vec![0]);
    }
}
