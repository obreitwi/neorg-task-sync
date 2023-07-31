use once_cell::unsync::Lazy;
use std::collections::HashMap;
use std::fs::read_to_string;
use std::fs::rename;
use std::fs::write;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use tree_sitter::Node;
use tree_sitter::Parser;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use crate::cfg::CFG;
use crate::error::WrapError;
use crate::Error;

const QUERY_TODO: Lazy<Rc<str>> = Lazy::new(|| {
    let todo_section_header = &CFG.todo_section_header;
    format!(
        r###"
(
 (unordered_list1
   state: (detached_modifier_extension [(todo_item_undone) (todo_item_done) (todo_item_pending)] @state )
   content: (paragraph (paragraph_segment . (inline_comment . ("_open") . ("_word" @task-id-tag) . ("_word" @task-id-content) .  ("_close") . ))) @content
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

#[derive(Debug)]
pub struct Todo {
    pub content: Rc<str>,
    pub id: Option<Rc<str>>,
    pub line: usize,
    pub state: State,
    pub bytes: TodoBytes,
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

#[derive(Debug)]
pub struct TodoBytes {
    pub state_start: usize,
    pub state_end: usize,
}

#[derive(Debug, PartialEq)]
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
}

#[derive(Debug)]
pub struct ParsedNorg {
    pub source_code: Vec<u8>,
    pub todos: Vec<Todo>,
    pub line_no: LineNo,
    filename: PathBuf,
}

#[derive(Debug)]
pub struct LineNo {
    pub todo_section: usize,
    pub section_after_todo: usize,
}

impl ParsedNorg {
    pub fn lines(&self) -> Vec<Vec<u8>> {
        self.source_code[..]
            .split(|t| *t == ('\n' as u8))
            .map(Vec::from)
            .collect()
    }

    pub fn write(&self) -> Result<(), Error> {
        rename(&self.filename, self.filename.with_extension("norg.bak"))?;
        write(&self.filename, &self.source_code[..])?;

        Ok(())
    }

    pub fn parse(file: &Path) -> Result<Self, Error> {
        let (query, idx) = get_query()?;

        let mut todos = HashMap::new();

        let mut parser = Parser::new();
        parser.set_language(tree_sitter_norg::language())?;

        let source_code = read_to_string(file).during("reading norg file")?;
        let tree = parser
            .parse(&source_code, None)
            .ok_or_else(|| Error::Parse)?;
        let source_code = source_code.into_bytes();
        // source_code.split(|s| *s == ('\n' as u8));

        log::debug!("Tree: {:#?}", tree);

        let mut cursor = QueryCursor::new();

        let get_content = |node: &Node| -> Result<Rc<str>, Error> {
            Ok(Rc::from(
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

                    let bytes = TodoBytes {
                        state_start: node_state.start_byte(),
                        state_end: node_state.end_byte(),
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
                            let content: Rc<str> = {
                                let content = get_content(&node_content)?;
                                content
                                    .split_once("%")
                                    .map(|(s, _)| Rc::from(s.trim()))
                                    .unwrap_or(content)
                            };

                            Todo {
                                line: node_state.start_position().row,
                                id: Some(id),
                                content,
                                state,
                                bytes,
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
        Ok(ParsedNorg {
            todos,
            source_code,
            line_no: LineNo {
                todo_section: line_no_todo_section,
                section_after_todo: line_no_sections
                    .into_iter()
                    .filter(|l| *l > line_no_todo_section)
                    .max()
                    .unwrap_or(usize::MAX),
            },
            filename: file.into(),
        })
    }
}

fn get_query() -> Result<(Query, QueryIndices), Error> {
    let query = Query::new(tree_sitter_norg::language(), &QUERY_TODO.clone())?;

    let indices = QueryIndices {
        content: query.capture_index_for_name("content").unwrap(),
        id_content: query.capture_index_for_name("task-id-content").unwrap(),
        // id_tag: query.capture_index_for_name("task-id-tag").unwrap(),
        state: query.capture_index_for_name("state").unwrap(),
    };

    Ok((query, indices))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn build_query() {
        get_query().unwrap();
    }
}
