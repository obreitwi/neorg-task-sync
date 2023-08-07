use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::read_to_string;
use std::fs::rename;
use std::fs::write;
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
    let todo_section_header = &CFG.todo_section_header;
    format!(
        r###"
(
 (unordered_list1
   state: (detached_modifier_extension [(todo_item_undone) (todo_item_done) (todo_item_pending)] @state )
   content: (paragraph (paragraph_segment (inline_comment . ("_open") . ("_word" @task-id-tag) . ("_word" @task-id-content) .  ("_close") . ) @task-id-comment)) @content
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
 (#match? @title "{todo_section_header}")
)
(
 (heading1
   title: (_) @title
 )
)
"###
    ).as_str().into()
});

const TODO_WITH_TAG: usize = 0;
const TODO_WITHOUT_TAG: usize = 1;
const TODO_SECTION: usize = 2;
const OTHER_SECTION: usize = 3;

#[derive(Debug, Clone)]
pub struct Todo {
    pub content: Arc<str>,
    pub id: Option<Arc<str>>,
    pub line: usize,
    pub state: State,
    // bytes are only valid if source code is not modified
    pub bytes: TodoBytes,
    // positions that operate in a single line, points stay valid until the given line is modified
    pub in_line: TodoInLine,
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
}

#[derive(Debug, Clone, Copy)]
pub struct TodoBytes {
    pub state: ByteRange,
    pub id_comment: Option<ByteRange>,
}

#[derive(Debug, Clone, Copy)]
pub struct TodoInLine {
    pub state: InLineRange,
    pub id_comment: Option<InLineRange>,
}

#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Clone, Copy)]
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

struct QueryIndices {
    content: u32,
    id_content: u32,
    // id_tag: u32,
    state: u32,
    id_comment: u32,
}

#[derive(Debug, Default)]
pub struct ParsedNorg {
    source_code: Vec<u8>,
    pub todos: Vec<Todo>,
    pub line_no: LineNo,
    pub filename: PathBuf,
}

#[derive(Debug)]
pub struct LineNo {
    pub todo_section: usize,
    pub section_after_todo: usize,
}

impl Default for LineNo {
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

    pub fn write(&self) -> Result<(), Error> {
        rename(&self.filename, self.filename.with_extension("norg.bak"))?;
        write(&self.filename, &self.source_code[..])?;

        Ok(())
    }

    pub fn open(file: &Path) -> Result<Self, Error> {
        let source_code = read_to_string(file).during("reading norg file")?;

        let mut new = ParsedNorg {
            filename: file.into(),
            ..Self::default()
        };
        new.reparse(source_code.as_bytes().to_vec())?;
        Ok(new)
    }

    pub fn reparse(&mut self, source_code: Vec<u8>) -> Result<(), Error> {
        let (query, idx) = get_query()?;

        let mut todos = HashMap::new();

        let mut parser = Parser::new();
        parser.set_language(tree_sitter_norg::language())?;

        let tree = parser
            .parse(&source_code[..], None)
            .ok_or_else(|| Error::Parse)?;

        log::debug!("Tree: {:#?}", tree);

        let mut cursor = QueryCursor::new();

        let get_content = |node: &Node| -> Result<Arc<str>, Error> {
            Ok(Arc::from(
                std::str::from_utf8(&source_code[node.start_byte()..node.end_byte()])?.trim(),
            ))
        };

        let mut line_no_todo_section = usize::MAX;
        let mut line_no_sections: Vec<usize> = Vec::new();

        for (i, m) in cursor
            .matches(&query, tree.root_node(), &source_code[..])
            .enumerate()
        {
            match m.pattern_index {
                TODO_WITH_TAG | TODO_WITHOUT_TAG => {
                    log::debug!("Match #{i} [{type}]: {m:#?}", type=if m.pattern_index == TODO_WITH_TAG {
                    "with tag"
                    } else {
                    "without tag"
                    });

                    let node_state = m
                        .nodes_for_capture_index(idx.state)
                        .next()
                        .expect("no node for state");
                    let state = State::from_kind(node_state.kind());

                    let node_comment = m.nodes_for_capture_index(idx.id_comment).next();

                    let bytes = TodoBytes {
                        state: node_state.into(),
                        id_comment: node_comment.map(|n| n.into()),
                    };
                    let in_line = TodoInLine {
                        state: node_state.into(),
                        id_comment: node_comment.map(|n| n.into()),
                    };

                    let todo = match m.pattern_index {
                        TODO_WITH_TAG => {
                            let node_id = m
                                .nodes_for_capture_index(idx.id_content)
                                .next()
                                .expect("no node for tag content");
                            let id = get_content(&node_id)?;

                            let node_content = m
                                .nodes_for_capture_index(idx.content)
                                .next()
                                .expect("no node for content");
                            let content: Arc<str> = {
                                let content = get_content(&node_content)?;
                                content
                                    .split_once('%')
                                    .map(|(s, _)| Arc::from(s.trim()))
                                    .unwrap_or(content)
                            };

                            Todo {
                                line: node_state.start_position().row,
                                id: Some(id),
                                content,
                                state,
                                bytes,
                                in_line,
                            }
                        }
                        TODO_WITHOUT_TAG => {
                            let node = m
                                .nodes_for_capture_index(idx.content)
                                .next()
                                .expect("no content nodes");
                            let line = node.start_position().row;
                            if todos.contains_key(&line) {
                                continue;
                            }
                            let content = get_content(&node)?;

                            Todo {
                                line: node_state.start_position().row,
                                id: None,
                                content,
                                state,
                                bytes,
                                in_line,
                            }
                        }

                        other => panic!("invalid pattern index when parsing todo: {other}"),
                    };
                    log::debug!("Inserting: {todo:?}");
                    todos.insert(todo.line, todo);
                }

                TODO_SECTION => {
                    line_no_todo_section = m.captures[0].node.start_position().row;
                }

                OTHER_SECTION => {
                    line_no_sections.push(m.captures[0].node.start_position().row);
                }
                other => panic!("invalid pattern index: {other}"),
            }
        }

        let mut todos = todos.into_values().collect::<Vec<_>>();
        todos.sort_by_key(|t| t.line);

        self.todos = todos;
        self.source_code = source_code;

        self.line_no = LineNo {
            todo_section: line_no_todo_section,
            section_after_todo: line_no_sections
                .into_iter()
                .filter(|l| *l > line_no_todo_section)
                .max()
                .unwrap_or(usize::MAX),
        };

        Ok(())
    }

    pub fn mark_completed(&mut self, idx: usize) {
        let todo = &self.todos[idx];
        let len_state = todo.bytes.state.end - todo.bytes.state.start;
        if len_state != 1 {
            log::warn!(
                "expected single byte for state char, found {} bytes",
                len_state
            );
        }

        self.source_code.splice(
            todo.bytes.state.start..todo.bytes.state.end,
            [b'x'].into_iter(),
        );
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

        self.reparse(source_code)
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
    };

    Ok((QUERY.clone(), indices))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn build_query() {
        get_query().unwrap();
    }
}
