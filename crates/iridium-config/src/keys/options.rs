//! The inline-table form adds mode transitions and count prefixes to bindings.

use iridium_editor::KeyBinding;
use iridium_editor::commands::ModeName;
use serde::Deserialize;
use toml::Value;

use crate::problem::Problem;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    command: Option<String>,
    #[serde(default)]
    then: Vec<String>,
    mode: Option<String>,
    enter_mode: Option<String>,
    #[serde(default)]
    count: bool,
    #[serde(default)]
    record_input: bool,
}

pub(super) fn binding(sequence: &str, value: &Value) -> Result<KeyBinding, Problem> {
    let options: Options = value.clone().try_into().map_err(|error| {
        Problem::keys(format!("`{sequence}` has invalid binding options: {error}"))
    })?;
    let mode = mode_name(sequence, "mode", options.mode)?;
    let enter_mode = mode_name(sequence, "enter_mode", options.enter_mode)?;
    let mut binding = if let Some(command) = options.command {
        if command.is_empty() && (enter_mode.is_some() || options.count) {
            return Err(Problem::keys(format!(
                "`{sequence}` cannot combine an unbinding with a mode transition or count"
            )));
        }
        super::binding(sequence, &Value::String(command))?
    } else if let Some(target) = enter_mode.clone() {
        let (first, rest) = KeyBinding::parse_sequence(sequence)
            .map_err(|error| super::unparsable(sequence, &error))?;
        KeyBinding::enter_mode(first, &rest, target)
    } else {
        return Err(Problem::keys(format!(
            "`{sequence}` needs `command` or `enter_mode`"
        )));
    };
    if !options.then.is_empty() {
        if binding.command().is_none() || options.then.len() >= 128 {
            return Err(Problem::keys(format!(
                "`{sequence}` needs a first command and at most 127 following commands"
            )));
        }
        for command in options.then {
            if command.trim().is_empty() {
                return Err(Problem::keys(format!(
                    "`{sequence}` has an empty following command"
                )));
            }
            binding = binding.then_command(iridium_editor::CommandId::new(command));
        }
    }
    if let Some(mode) = mode {
        binding = binding.in_mode(mode);
    }
    if options.record_input && (enter_mode.is_none() || binding.command().is_none()) {
        return Err(Problem::keys(format!(
            "`{sequence}`: recording requires a command and enter_mode"
        )));
    }
    if let Some(target) = enter_mode {
        binding = if options.record_input {
            binding.then_record_input(target)
        } else {
            binding.then_enter_mode(target)
        };
    }
    if options.count {
        if binding.command().is_none() {
            return Err(Problem::keys(format!(
                "`{sequence}` needs a command to accept a count"
            )));
        }
        binding = binding.with_count_prefix();
    }
    Ok(binding)
}

fn mode_name(
    sequence: &str,
    field: &str,
    value: Option<String>,
) -> Result<Option<ModeName>, Problem> {
    value
        .map(|name| {
            if name.trim().is_empty() || name.trim() != name {
                Err(Problem::keys(format!(
                    "`{sequence}`: `{field}` must be nonempty without surrounding whitespace"
                )))
            } else {
                Ok(ModeName::new(name))
            }
        })
        .transpose()
}
