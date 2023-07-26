use std::fs::read_to_string;
use std::path::Path;
use tree_sitter::Parser;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use crate::error::WrapError;
use crate::Error;

const QUERY_TODO: &str = r###"
(
 (unordered_list1
   state: (detached_modifier_extension [(todo_item_undone) (todo_item_done) (todo_item_pending)])
   content: (paragraph (paragraph_segment (inline_comment ("_open") ("_word" @task-id-tag) ("_word") ("_close")) @conceal (#set! conceal "")))
 )
 (#match? @task-id-tag "#taskid")
)
"###;

pub fn parse_norg(file: &Path) -> Result<(), Error> {
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_norg::language())?;

    let source_code = read_to_string(file).during("reading norg file")?;

    let tree = parser
        .parse(&source_code, None)
        .ok_or_else(|| Error::Parse)?;

    log::debug!("Tree: {:#?}", tree);

    let query = Query::new(tree_sitter_norg::language(), QUERY_TODO)?;

    let mut cursor = QueryCursor::new();

    for (i, m) in cursor
        .matches(&query, tree.root_node(), &source_code.into_bytes()[..])
        .enumerate()
    {
        log::debug!("Capture #{i}: {m:#?}");
    }

    Ok(())
}
