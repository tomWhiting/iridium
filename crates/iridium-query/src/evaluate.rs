//! A pure jaq runtime: no filesystem/module loader, environment or input streams.
use super::source::Index;
use crate::{Format, MAX_MATCHES, Query, QueryResult};
use jaq_core::{
    Compiler, Ctx, Vars, data,
    load::{Arena, File, Loader},
    unwrap_valr,
};
use jaq_json::{Val, read};

pub fn select(source: &str, query: &Query) -> QueryResult {
    let program = format!("path({})", query.filter);
    let definitions = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let functions = jaq_core::funs::<data::JustLut<Val>>()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let arena = Arena::default();
    let modules = Loader::new(definitions)
        .load(
            &arena,
            File {
                code: &program,
                path: (),
            },
        )
        .map_err(|errors| format!("jq parse error: {errors:?}"))?;
    let compiled = Compiler::default()
        .with_funs(functions)
        .compile(modules)
        .map_err(|errors| format!("jq compile error: {errors:?}"))?;
    let format = match query.format {
        Format::Auto if serde_json::from_str::<serde_json::Value>(source).is_ok() => Format::Json,
        Format::Auto => Format::JsonLines,
        value => value,
    };
    let records: Box<dyn Iterator<Item = &str> + '_> = if format == Format::JsonLines {
        Box::new(source.split_inclusive('\n'))
    } else {
        Box::new(std::iter::once(source))
    };
    let mut offset = 0;
    let mut nodes = 0;
    let mut output = Vec::new();
    let mut seen = 0;
    for (record, text) in records.enumerate() {
        seen += 1;
        let context = |error: String| {
            if format == Format::JsonLines {
                format!("JSONL line {}: {error}", record + 1)
            } else {
                error
            }
        };
        let index = Index::parse(text, offset, &mut nodes).map_err(&context)?;
        let input = read::parse_single(text.as_bytes())
            .map_err(|error| context(format!("jq input: {error:?}")))?;
        let ctx = Ctx::<data::JustLut<Val>>::new(&compiled.lut, Vars::new([]));
        for value in compiled.id.run((ctx, input)).map(unwrap_valr) {
            let value = value.map_err(|error| context(format!("jq runtime error: {error}")))?;
            if output.len() == MAX_MATCHES {
                return Err(format!(
                    "jq exceeds {MAX_MATCHES} matches; narrow the filter"
                ));
            }
            let path = serde_json::from_str(&value.to_string())
                .map_err(|error| context(error.to_string()))?;
            output.push(index.resolve(&path, query.target).map_err(&context)?);
        }
        offset += text.len();
    }
    if seen == 0 {
        return Err("JSON input is empty".into());
    }
    output.sort_unstable();
    output.dedup();
    Ok(output)
}
