use chrono::DateTime;
use chrono::Local;
use chrono::NaiveDate;
use chrono::NaiveDateTime;
use chrono::NaiveTime;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tree_sitter::Node;
use tree_sitter::Parser;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use crate::cfg::CFG;
use crate::error::WrapError;
use crate::Error;

static QUERY_TODO: Lazy<Arc<str>> = Lazy::new(|| {
    r##"
(
 (unordered_list1
   state: (detached_modifier_extension [(todo_item_undone) (todo_item_done) (todo_item_pending)] @state )
   content: (paragraph (paragraph_segment (inline_comment . ("_open") . ("_word" @task-id-tag) . ("_word" @task-id-content) .  ("_close") . ) @task-id-comment) @content )
 )
 (#match? @task-id-tag "#taskid")
)
(
 (unordered_list1
   state: (detached_modifier_extension [(todo_item_undone) (todo_item_done) (todo_item_pending)] @state)
   content: (paragraph (paragraph_segment)) @content
 )
)
(
 (heading1
   title: (_) @title
 )
)
"##.into()
});

const TODO_WITH_TAG: usize = 0;
const TODO_WITHOUT_TAG: usize = 1;
const TODO_SECTION: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Todo {
    pub content: Arc<str>,
    pub id: Option<Arc<str>>,
    pub line: usize,
    pub state: State,
    // bytes are only valid if source code is not modified
    pub bytes: TodoBytes,
    // positions that operate in a single line, points stay valid until the given line is modified
    pub in_line: TodoInLine,
    pub due_at: Option<NaiveDate>,
}

impl Todo {
    pub fn append_id(&self, line: &mut Vec<u8>) {
        line.extend(
            format!(
                " %#taskid {}%",
                self.id.clone().expect("no todo id to append")
            )
            .into_bytes(),
        )
    }

    pub fn due_at_fmt(&self) -> Option<String> {
        self.due_at.map(|d| {
            NaiveDateTime::new(d, NaiveTime::default())
                .and_utc()
                .to_rfc3339()
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TodoBytes {
    pub content: ByteRange,
    pub id_comment: Option<ByteRange>,
    pub state: ByteRange,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TodoInLine {
    pub content: InLineRange,
    pub id_comment: Option<InLineRange>,
    pub state: InLineRange,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl From<Node<'_>> for ByteRange {
    fn from(n: Node) -> Self {
        Self::from(&n)
    }
}
impl From<&Node<'_>> for ByteRange {
    fn from(n: &Node) -> Self {
        Self {
            start: n.start_byte(),
            end: n.end_byte(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InLineRange {
    pub start: usize,
    pub end: usize,
}

impl From<Node<'_>> for InLineRange {
    fn from(n: Node) -> Self {
        Self::from(&n)
    }
}
impl From<&Node<'_>> for InLineRange {
    fn from(n: &Node) -> Self {
        Self {
            start: n.start_position().column,
            end: n.end_position().column,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum State {
    Undone,
    Pending,
    Done,
}

impl State {
    // will panic if kind is wrong
    fn from_kind(k: &str) -> State {
        match k {
            "todo_item_undone" => State::Undone,
            "todo_item_pending" => State::Pending,
            "todo_item_done" => State::Done,
            other => panic!("invalid kind: {other}"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct QueryIndices {
    content: u32,
    id_comment: u32,
    id_content: u32,
    // id_tag: u32,
    state: u32,
    title: u32,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct ParsedNorg {
    source_code: Vec<u8>,
    pub todos: Vec<Todo>,
    pub line_number: LineNumbers,
    pub filename: PathBuf,
    pub modified_at: DateTime<Local>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct LineNumbers {
    pub todo_section: usize,
    pub section_after_todo: usize,
}

impl Default for LineNumbers {
    fn default() -> Self {
        Self {
            todo_section: usize::MAX,
            section_after_todo: usize::MAX,
        }
    }
}

impl ParsedNorg {
    pub fn lines(&self) -> Vec<Vec<u8>> {
        self.source_code[..]
            .split(|t| *t == (b'\n'))
            .map(Vec::from)
            .collect()
    }

    pub fn backup(&self) -> Result<(), Error> {
        let full = fs::canonicalize(&self.filename)?;
        let full_name = full.to_string_lossy().replace('/', "%");
        let copy_to = env::temp_dir().join(format!("neorg_task_sync_{full_name}"));
        fs::copy(&self.filename, copy_to)?;
        Ok(())
    }

    pub fn write(&self) -> Result<(), Error> {
        fs::write(&self.filename, &self.source_code[..])?;
        Ok(())
    }

    pub fn idx_by_todo_id(&self, id: &str) -> usize {
        self.todos
            .iter()
            .enumerate()
            .find_map(|(idx, t)| {
                if t.id.is_some() && t.id.clone().unwrap().as_ref() == id {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap()
    }

    pub fn open(file: &Path) -> Result<Self, Error> {
        let metadata = fs::metadata(file).during("reading metadata")?;
        let source_code = fs::read_to_string(file).during("reading norg file")?;

        let mut new = ParsedNorg {
            filename: file.into(),
            modified_at: metadata
                .modified()
                .during("getting modification date")?
                .into(),
            ..Self::default()
        };
        new.reparse(source_code.as_bytes().to_vec())?;
        Ok(new)
    }

    // Get day that this file governs, if it's parseable
    fn parse_filename_day(&self) -> Result<NaiveDate, Error> {
        Ok(NaiveDate::parse_from_str(
            &self
                .filename
                .with_extension("")
                .file_name()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default(),
            "%Y-%m-%d",
        )
        .during("parsing filename as date")?)
    }

    pub fn reparse(&mut self, source_code: Vec<u8>) -> Result<(), Error> {
        let (query, idx) = get_query()?;

        let mut todos = HashMap::new();

        let mut parser = Parser::new();
        parser.set_language(tree_sitter_norg::language())?;

        let tree = parser
            .parse(&source_code[..], None)
            .ok_or_else(|| Error::Parse)?;

        log::debug!("Tree: {tree:#?}");

        let mut cursor = QueryCursor::new();

        let get_content = |node: &Node| -> Result<Arc<str>, Error> {
            Ok(Arc::from(
                std::str::from_utf8(&source_code[node.start_byte()..node.end_byte()])?.trim(),
            ))
        };

        let mut section_to_line: HashMap<Arc<str>, usize> = HashMap::new();

        for (i, m) in cursor
            .matches(&query, tree.root_node(), &source_code[..])
            .enumerate()
        {
            match m.pattern_index {
                TODO_WITH_TAG | TODO_WITHOUT_TAG => {
                    if log::log_enabled!(log::Level::Debug) {
                        log::debug!("Match #{i} [{type}]: {m:#?}", type=if m.pattern_index == TODO_WITH_TAG {
                        "with tag"
                        } else {
                        "without tag"
                        });
                    }

                    let node_state = m
                        .nodes_for_capture_index(idx.state)
                        .next()
                        .expect("no node for state");
                    let state = State::from_kind(node_state.kind());

                    let node_comment = m.nodes_for_capture_index(idx.id_comment).next();

                    let node_content = m
                        .nodes_for_capture_index(idx.content)
                        .next()
                        .expect("no node for content");

                    let mut bytes = TodoBytes {
                        content: node_content.into(),
                        id_comment: node_comment.map(|n| n.into()),
                        state: node_state.into(),
                    };
                    let mut in_line = TodoInLine {
                        content: node_content.into(),
                        id_comment: node_comment.map(|n| n.into()),
                        state: node_state.into(),
                    };

                    let line = node_state.start_position().row;

                    let todo = match m.pattern_index {
                        TODO_WITH_TAG => {
                            let node_id = m
                                .nodes_for_capture_index(idx.id_content)
                                .next()
                                .expect("no node for tag content");
                            let id = get_content(&node_id)?;

                            let content: Arc<str> = {
                                let content = get_content(&node_content)?;
                                content
                                    .split_once('%')
                                    .map(|(s, _)| Arc::from(s.trim()))
                                    .unwrap_or(content)
                            };

                            bytes.content.end = bytes.id_comment.as_ref().unwrap().start - 1;
                            in_line.content.end = in_line.id_comment.as_ref().unwrap().start - 1;

                            Todo {
                                line,
                                id: Some(id),
                                content,
                                state,
                                bytes,
                                in_line,
                                due_at: None,
                            }
                        }
                        TODO_WITHOUT_TAG => {
                            if todos.contains_key(&line) {
                                continue;
                            }
                            let content = get_content(&node_content)?;

                            Todo {
                                line,
                                id: None,
                                content,
                                state,
                                bytes,
                                in_line,
                                due_at: None,
                            }
                        }

                        other => panic!("invalid pattern index when parsing todo: {other}"),
                    };
                    log::debug!("Inserting: {todo:?}");
                    todos.insert(todo.line, todo);
                }

                TODO_SECTION => {
                    let node_title = m
                        .nodes_for_capture_index(idx.title)
                        .next()
                        .expect("no node for title");
                    let line = node_title.start_position().row;
                    let title = get_content(&node_title)?;
                    section_to_line.insert(title, line);
                }

                other => panic!("invalid pattern index: {other}"),
            }
        }

        let mut todos = todos.into_values().collect::<Vec<_>>();
        todos.sort_by_key(|t| t.line);

        self.todos = todos;
        self.source_code = source_code;

        let header: Arc<str> = CFG.section_todos.clone();

        let line_todo_section = section_to_line.get(&header).cloned().unwrap_or(usize::MAX);
        self.line_number = LineNumbers {
            todo_section: line_todo_section,
            section_after_todo: section_to_line
                .values()
                .filter(|l| **l > line_todo_section)
                .min()
                .cloned()
                .unwrap_or(usize::MAX),
        };

        match self.set_due_date(&section_to_line) {
            Ok(()) => {}
            Err(Error::NotFound { what }) => {
                log::debug!(
                    "did not find {what} in {file}",
                    file = self.filename.display()
                );
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn update_task_titles<S, I>(&mut self, items: I) -> Result<(), Error>
    where
        S: ToString,
        I: IntoIterator<Item = (usize, S)>,
    {
        let mut lines = self.lines();

        for (idx, s) in items.into_iter() {
            let todo = &self.todos[idx];
            let range = todo.in_line.content;
            lines[todo.line].splice(
                range.start..range.end,
                format!(" {}", s.to_string()).bytes(),
            );
        }

        self.set_lines(&lines[..])?;
        Ok(())
    }

    pub fn mark_completed(&mut self, idx: usize) {
        let todo = &self.todos[idx];
        let len_state = todo.bytes.state.end - todo.bytes.state.start;
        if len_state != 1 {
            log::warn!("expected single byte for state char, found {len_state} bytes",);
        }

        self.source_code
            .splice(todo.bytes.state.start..todo.bytes.state.end, [b'x']);
    }

    // clear tags for all todo indices listed
    pub fn clear_tags(&mut self, indices: &[usize]) -> Result<(), Error> {
        let mut lines = self.lines();

        for (idx, line) in lines
            .iter_mut()
            .enumerate()
            .filter(|(idx, _line)| indices.contains(idx))
        {
            let todo = &mut self.todos[idx];

            if todo.in_line.id_comment.is_none() {
                let title = todo.content.clone();
                log::warn!("Todo entry '{title}' does not contain a tag, skipping…");
                continue;
            }

            let in_line = todo.in_line.id_comment.unwrap();
            line.splice(in_line.start - 1..in_line.end, []);
        }
        self.set_lines(&lines[..])
    }

    pub fn set_lines<'a, L, I>(&mut self, lines: L) -> Result<(), Error>
    where
        L: IntoIterator<Item = I>,
        I: IntoIterator<Item = &'a u8>,
    {
        let mut source_code = Vec::new();
        for l in lines.into_iter() {
            source_code.extend(l.into_iter());
            source_code.push(b'\n');
        }

        match source_code.pop() {
            Some(b'\n') | None => {}
            Some(c) => source_code.push(c),
        }

        self.reparse(source_code)
    }

    // set due date if requested
    fn set_due_date(&mut self, section_to_line: &HashMap<Arc<str>, usize>) -> Result<(), Error> {
        if CFG.section_todos_till_end_of_day.is_none() {
            return Ok(());
        }
        let header = CFG.section_todos_till_end_of_day.clone().unwrap();

        let line_header = section_to_line
            .get(&header)
            .cloned()
            .ok_or_else(|| Error::NotFound {
                what: format!("section: {header}"),
            })?;

        let line_next = section_to_line
            .values()
            .filter(|l| **l > line_header)
            .min()
            .cloned()
            .unwrap_or(usize::MAX);

        let day = self.parse_filename_day()?;

        for todo in self
            .todos
            .iter_mut()
            .filter(|t| line_header < t.line && t.line < line_next)
        {
            todo.due_at = Some(day);
        }

        // During sync:
        // Check if due date differs -> Update

        Ok(())
    }
}

fn get_query() -> Result<(Arc<Query>, QueryIndices), Error> {
    static QUERY: Lazy<Arc<Query>> = Lazy::new(|| {
        Arc::new(
            Query::new(tree_sitter_norg::language(), &QUERY_TODO.clone())
                .expect("could not parse query"),
        )
    });
    let indices = QueryIndices {
        content: QUERY.capture_index_for_name("content").unwrap(),
        id_content: QUERY.capture_index_for_name("task-id-content").unwrap(),
        id_comment: QUERY.capture_index_for_name("task-id-comment").unwrap(),
        // id_tag: query.capture_index_for_name("task-id-tag").unwrap(),
        state: QUERY.capture_index_for_name("state").unwrap(),
        title: QUERY.capture_index_for_name("title").unwrap(),
    };

    Ok((QUERY.clone(), indices))
}

#[cfg(test)]
mod test {
    use super::*;

    use pretty_assertions::assert_eq;

    static TEMP_NORG_GIVEN: &str = r###"

* TODOs
  - ( ) This is a test %#taskid foobar1%
  - ( ) This is yet another test %#taskid foobar2%
  - ( ) And for good measure a third task %#taskid foobar3%

"###;
    static TEMP_NORG_WANT: &str = r###"

* TODOs
  - ( ) This is a test %#taskid foobar1%
  - ( ) this is a test %#taskid foobar2%
  - ( ) another test %#taskid foobar3%


"###;

    #[test]
    fn build_query() {
        get_query().unwrap();
    }

    #[test]
    fn update_task_titles() -> Result<(), Error> {
        let filename = std::env::temp_dir().join("temp.norg");
        fs::write(&filename, TEMP_NORG_GIVEN)?;
        let mut norg = ParsedNorg::open(&filename)?;
        norg.update_task_titles([(2, "another test"), (1, "this is a test")])?;
        norg.write()?;

        let got = fs::read_to_string(filename)?;

        assert_eq!(got, TEMP_NORG_WANT);

        Ok(())
    }
}
