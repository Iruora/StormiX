//! Hands out test data rows to load test iterations and virtual users.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use stormix_flow::{parse_csv, DataSet, Per, Pick};

/// The test file's `data` sets, read and ready to pick from. Shared by every
/// iteration of a load test; picking is lock-free.
#[derive(Debug, Default)]
pub struct DataFeed {
    sets: Vec<Feed>,
}

#[derive(Debug)]
struct Feed {
    label: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    pick: Pick,
    per: Per,
    next: AtomicUsize,
    counts: Vec<AtomicU64>,
    ran_out: AtomicBool,
}

/// How one data set was used in a load test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DataUse {
    /// The set's variables, or its file.
    pub label: String,
    pub columns: Vec<String>,
    pub rows: u32,
    pub pick: Pick,
    pub per: Per,
    /// Rows handed out: one per iteration, or per user with `per: user`.
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub picks: u64,
    /// `pick: unique` only: every row was used, so the test ended early.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ran_out: bool,
    /// How often each row was picked, in file order. Left out for sets of
    /// more than [`MAX_ROW_COUNTS`] rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub counts: Vec<RowCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RowCount {
    /// The row's values, in column order.
    pub values: Vec<String>,
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub count: u64,
}

/// Larger sets report only their totals.
pub const MAX_ROW_COUNTS: usize = 50;

/// One picked row: variable names and values.
pub type Row = Vec<(String, String)>;

/// Rows picked from some of the sets, by set and row index. Turned into
/// variables with [`DataFeed::row`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picks(Vec<(usize, usize)>);

impl Picks {
    /// These picks and `other`'s together.
    pub fn and(mut self, other: &Picks) -> Self {
        self.0.extend_from_slice(&other.0);
        self
    }
}

impl DataFeed {
    /// Reads `sets`, files relative to `base` (the test file's folder).
    pub fn load(sets: &[DataSet], base: Option<&Path>) -> Result<Self, String> {
        let mut feeds = Vec::with_capacity(sets.len());
        for set in sets {
            let (columns, rows) = match &set.file {
                Some(file) => {
                    let path = Path::new(file);
                    let path = match base {
                        Some(base) if path.is_relative() => base.join(path),
                        _ => path.to_path_buf(),
                    };
                    let text = std::fs::read_to_string(&path).map_err(|e| {
                        format!("cannot read the test data file {}: {e}", path.display())
                    })?;
                    parse_csv(&text).map_err(|e| format!("{}: {e}", path.display()))?
                }
                None => (set.columns(), set.inline_rows()),
            };
            if rows.is_empty() {
                return Err(format!("the test data `{}` has no rows", set.label()));
            }
            feeds.push(Feed {
                label: set.label(),
                counts: rows.iter().map(|_| AtomicU64::new(0)).collect(),
                columns,
                rows,
                pick: set.pick,
                per: set.per,
                next: AtomicUsize::new(0),
                ran_out: AtomicBool::new(false),
            });
        }
        Ok(Self { sets: feeds })
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// The first row of every set, for a run once. Not counted.
    pub fn first_row(&self) -> Row {
        self.sets
            .iter()
            .flat_map(|f| f.columns.iter().cloned().zip(f.rows[0].iter().cloned()))
            .collect()
    }

    /// A row from every set picked `per` that, or from every set when
    /// `per` is `None`. `None` once a `unique` set has used every row;
    /// nothing is counted then.
    pub fn pick(&self, per: Option<Per>) -> Option<Picks> {
        let mut picks = Vec::new();
        for (set, feed) in self.sets.iter().enumerate() {
            if per.is_some_and(|p| feed.per != p) {
                continue;
            }
            let index = match feed.pick {
                Pick::Random => fastrand::usize(..feed.rows.len()),
                Pick::Sequential => feed.next.fetch_add(1, Ordering::Relaxed) % feed.rows.len(),
                Pick::Unique => {
                    let i = feed.next.fetch_add(1, Ordering::Relaxed);
                    if i >= feed.rows.len() {
                        feed.ran_out.store(true, Ordering::Relaxed);
                        return None;
                    }
                    i
                }
            };
            picks.push((set, index));
        }
        for &(set, index) in &picks {
            self.sets[set].counts[index].fetch_add(1, Ordering::Relaxed);
        }
        Some(Picks(picks))
    }

    /// The variables of `picks`, set by set in file order, so a later set
    /// wins where two fill the same variable.
    pub fn row(&self, picks: &Picks) -> Row {
        let mut picks = picks.0.clone();
        picks.sort_unstable_by_key(|&(set, _)| set);
        picks
            .into_iter()
            .flat_map(|(set, index)| {
                let feed = &self.sets[set];
                feed.columns
                    .iter()
                    .cloned()
                    .zip(feed.rows[index].iter().cloned())
            })
            .collect()
    }

    /// How each set was used so far.
    pub fn usage(&self) -> Vec<DataUse> {
        self.sets
            .iter()
            .map(|f| {
                let counts: Vec<u64> = f.counts.iter().map(|c| c.load(Ordering::Relaxed)).collect();
                DataUse {
                    label: f.label.clone(),
                    columns: f.columns.clone(),
                    rows: f.rows.len() as u32,
                    pick: f.pick,
                    per: f.per,
                    picks: counts.iter().sum(),
                    ran_out: f.ran_out.load(Ordering::Relaxed),
                    counts: if f.rows.len() <= MAX_ROW_COUNTS {
                        f.rows
                            .iter()
                            .zip(counts)
                            .map(|(values, count)| RowCount {
                                values: values.clone(),
                                count,
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(json: &str) -> DataSet {
        serde_json::from_str(json).unwrap()
    }

    fn values(row: &Row) -> Vec<&str> {
        row.iter().map(|(_, v)| v.as_str()).collect()
    }

    #[test]
    fn sequential_wraps_and_unique_runs_out() {
        let feed = DataFeed::load(
            &[
                set(r#"{"variable": "id", "values": ["a", "b"], "pick": "sequential"}"#),
                set(r#"{"variable": "n", "values": [1, 2, 3], "pick": "unique"}"#),
            ],
            None,
        )
        .unwrap();
        assert_eq!(values(&feed.first_row()), ["a", "1"]);
        let picked: Vec<_> = (0..3)
            .map(|_| values(&feed.row(&feed.pick(None).unwrap())).join(""))
            .collect();
        assert_eq!(picked, ["a1", "b2", "a3"]);
        assert_eq!(feed.pick(None), None);
        let usage = feed.usage();
        assert_eq!(usage[0].picks, 3, "a pick that ran out counts for no set");
        assert!(!usage[0].ran_out);
        assert!(usage[1].ran_out);
        assert_eq!(usage[1].picks, 3);
        assert_eq!(
            usage[1].counts.iter().map(|c| c.count).collect::<Vec<_>>(),
            [1, 1, 1]
        );
    }

    #[test]
    fn random_uses_every_row_and_per_filters_sets() {
        let feed = DataFeed::load(
            &[
                set(r#"{"variable": "id", "values": ["a", "b", "c"]}"#),
                set(r#"{"rows": [{"user": "u1", "pass": "p1"}], "per": "user"}"#),
            ],
            None,
        )
        .unwrap();
        for _ in 0..300 {
            let row = feed.row(&feed.pick(Some(Per::Iteration)).unwrap());
            assert_eq!(row.len(), 1);
            assert_eq!(row[0].0, "id");
        }
        let user = feed.row(&feed.pick(Some(Per::User)).unwrap());
        assert_eq!(
            user,
            [("pass".into(), "p1".into()), ("user".into(), "u1".into())]
        );
        let usage = feed.usage();
        assert_eq!(usage[0].picks, 300);
        assert!(
            usage[0].counts.iter().all(|c| c.count > 50),
            "{:?}",
            usage[0].counts
        );
        assert_eq!(usage[1].picks, 1);
    }

    #[test]
    fn a_later_set_wins_whatever_its_scope() {
        let feed = DataFeed::load(
            &[
                set(r#"{"variable": "id", "values": ["user"], "per": "user"}"#),
                set(r#"{"variable": "id", "values": ["iteration"]}"#),
            ],
            None,
        )
        .unwrap();
        let user = feed.pick(Some(Per::User)).unwrap();
        let both = feed.pick(Some(Per::Iteration)).unwrap().and(&user);
        let row = feed.row(&both);
        assert_eq!(row.last().unwrap().1, "iteration");
    }

    #[test]
    fn reads_csv_files_next_to_the_test() {
        let dir = std::env::temp_dir().join(format!("stormix-data-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("users.csv"), "userId,name\n7,Ada\n8,Grace\n").unwrap();
        let feed = DataFeed::load(&[set(r#"{"file": "users.csv"}"#)], Some(&dir)).unwrap();
        assert_eq!(
            feed.first_row(),
            [("userId".into(), "7".into()), ("name".into(), "Ada".into())]
        );
        let err = DataFeed::load(&[set(r#"{"file": "missing.csv"}"#)], Some(&dir)).unwrap_err();
        assert!(err.contains("missing.csv"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
