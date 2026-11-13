//! Test data: lists of values that fill `{{name}}` differently from one
//! iteration (or virtual user) to the next, so a load test spreads over
//! many users, IDs or products instead of hitting one.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// One list of values. Either a single `variable` with its `values`,
/// several variables together as `rows`, or a CSV `file` whose header row
/// names the variables. A run once takes the first row; a load test picks
/// rows as `pick` and `per` say.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(deny_unknown_fields)]
pub struct DataSet {
    /// The variable `values` fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
    /// One value per row, for `variable`.
    #[serde(
        default,
        deserialize_with = "scalars",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[cfg_attr(feature = "specta", specta(type = Vec<String>))]
    pub values: Vec<String>,
    /// Rows of several variables that go together, like a user and their
    /// password.
    #[serde(
        default,
        deserialize_with = "scalar_rows",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[cfg_attr(feature = "specta", specta(type = Vec<BTreeMap<String, String>>))]
    pub rows: Vec<BTreeMap<String, String>>,
    /// A CSV file, relative to the test file. Its first line names the
    /// variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Pick::is_random")]
    pub pick: Pick,
    #[serde(default, skip_serializing_if = "Per::is_iteration")]
    pub per: Per,
}

/// How rows are chosen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    /// Any row, at random each time; rows repeat.
    #[default]
    Random,
    /// The rows in order, starting over after the last.
    Sequential,
    /// Each row once, in order. Once every row has been used the load
    /// test starts no more iterations and ends early.
    Unique,
}

/// When a new row is picked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum Per {
    /// Every iteration of the flow gets its own row.
    #[default]
    Iteration,
    /// `virtual_users` only: each user picks a row when it starts and
    /// keeps it for all its iterations. Under the other load profiles
    /// every iteration is a new user, so this is the same as `iteration`.
    User,
}

impl Pick {
    fn is_random(&self) -> bool {
        *self == Self::Random
    }
}

impl Per {
    fn is_iteration(&self) -> bool {
        *self == Self::Iteration
    }
}

impl DataSet {
    /// The variables an inline set fills, in column order. Empty for a file,
    /// whose columns are only known once it is read.
    pub fn columns(&self) -> Vec<String> {
        if let Some(variable) = &self.variable {
            return vec![variable.clone()];
        }
        let mut columns: Vec<String> = Vec::new();
        for row in &self.rows {
            for key in row.keys() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
        }
        columns
    }

    /// An inline set's rows as values in [`columns`](Self::columns) order.
    /// Empty for a file.
    pub fn inline_rows(&self) -> Vec<Vec<String>> {
        if self.variable.is_some() {
            return self.values.iter().map(|v| vec![v.clone()]).collect();
        }
        let columns = self.columns();
        self.rows
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|c| row.get(c).cloned().unwrap_or_default())
                    .collect()
            })
            .collect()
    }

    /// How the set is shown in messages: its variables, or its file.
    pub fn label(&self) -> String {
        match &self.file {
            Some(file) => file.clone(),
            None => self.columns().join(", "),
        }
    }
}

/// A CSV text's header and rows. Quoted fields may hold commas, quotes
/// (doubled) and line breaks; blank lines are skipped; a leading byte order
/// mark is ignored. Every row needs as many fields as the header.
pub fn parse_csv(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records: Vec<(usize, Vec<String>)> = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut quoted = false;
    let mut line = 1;
    let mut start_line = 1;
    let mut chars = text.chars().peekable();
    let mut touched = false;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line += 1;
                    field.push(c);
                }
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => {
                quoted = true;
                touched = true;
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                touched = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                if touched || !field.is_empty() {
                    record.push(std::mem::take(&mut field));
                    records.push((start_line, std::mem::take(&mut record)));
                }
                touched = false;
                line += 1;
                start_line = line;
            }
            _ => {
                field.push(c);
                touched = true;
            }
        }
    }
    if quoted {
        return Err(format!("line {start_line}: a quoted field is never closed"));
    }
    if touched || !field.is_empty() {
        record.push(field);
        records.push((start_line, record));
    }
    let mut records = records.into_iter();
    let Some((_, header)) = records.next() else {
        return Err("the file is empty; its first line names the variables".into());
    };
    let header: Vec<String> = header.into_iter().map(|h| h.trim().to_owned()).collect();
    if let Some(i) = header.iter().position(String::is_empty) {
        return Err(format!("line 1: column {} has no name", i + 1));
    }
    for (i, name) in header.iter().enumerate() {
        if header[..i].contains(name) {
            return Err(format!("line 1: two columns are named `{name}`"));
        }
    }
    let mut rows = Vec::new();
    for (line, row) in records {
        if row.len() != header.len() {
            return Err(format!(
                "line {line}: {} fields, but the header names {}",
                row.len(),
                header.len()
            ));
        }
        rows.push(row);
    }
    if rows.is_empty() {
        return Err("the file has a header but no rows".into());
    }
    Ok((header, rows))
}

/// YAML numbers and booleans in a value list, read as the text they show:
/// `values: [1, 2, 3]` are the IDs "1", "2" and "3".
fn scalars<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<String>, D::Error> {
    let values = Vec::<serde_json::Value>::deserialize(de)?;
    values
        .into_iter()
        .map(scalar_text)
        .collect::<Result<_, _>>()
        .map_err(serde::de::Error::custom)
}

fn scalar_rows<'de, D: Deserializer<'de>>(
    de: D,
) -> Result<Vec<BTreeMap<String, String>>, D::Error> {
    let rows = Vec::<BTreeMap<String, serde_json::Value>>::deserialize(de)?;
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|(k, v)| scalar_text(v).map(|v| (k, v)))
                .collect()
        })
        .collect::<Result<_, _>>()
        .map_err(serde::de::Error::custom)
}

fn scalar_text(value: serde_json::Value) -> Result<String, String> {
    use serde_json::Value;
    match value {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Null => Ok(String::new()),
        other => Err(format!(
            "a value is text or a number, not {}",
            if other.is_array() { "a list" } else { "a map" }
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_csv() {
        let (header, rows) = parse_csv(
            "\u{feff}id,name\r\n1,Ada\n\n2,\"Lovelace, \"\"Countess\"\"\nof Lovelace\"\n3,\n",
        )
        .unwrap();
        assert_eq!(header, ["id", "name"]);
        assert_eq!(
            rows,
            [
                vec!["1", "Ada"],
                vec!["2", "Lovelace, \"Countess\"\nof Lovelace"],
                vec!["3", ""],
            ]
        );
        let (header, rows) = parse_csv("userId\nu1\nu2").unwrap();
        assert_eq!(header, ["userId"]);
        assert_eq!(rows, [vec!["u1"], vec!["u2"]]);
    }

    #[test]
    fn csv_errors_name_the_line() {
        assert!(parse_csv("").unwrap_err().contains("empty"));
        assert!(parse_csv("id\n").unwrap_err().contains("no rows"));
        assert_eq!(
            parse_csv("a,b\n1,2\n3\n").unwrap_err(),
            "line 3: 1 fields, but the header names 2"
        );
        assert!(parse_csv("a,a\n1,2").unwrap_err().contains("two columns"));
        assert!(parse_csv("a,\n1,2").unwrap_err().contains("column 2"));
        assert!(parse_csv("a\n\"open").unwrap_err().contains("line 2"));
    }

    #[test]
    fn inline_sets_list_their_columns_and_rows() {
        let single = DataSet {
            variable: Some("userId".into()),
            values: vec!["1".into(), "2".into()],
            ..DataSet::default()
        };
        assert_eq!(single.columns(), ["userId"]);
        assert_eq!(single.inline_rows(), [vec!["1"], vec!["2"]]);
        let pairs = DataSet {
            rows: vec![
                BTreeMap::from([("user".into(), "a".into()), ("pass".into(), "x".into())]),
                BTreeMap::from([("user".into(), "b".into())]),
            ],
            ..DataSet::default()
        };
        assert_eq!(pairs.columns(), ["pass", "user"]);
        assert_eq!(pairs.inline_rows(), [vec!["x", "a"], vec!["", "b"]]);
    }
}
