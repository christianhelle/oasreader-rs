//! Prints an OpenAPI specification with its external references merged.
//!
//! ```text
//! cargo run --example merge -- <path-or-url> [--json | --yaml]
//! ```

use std::{env, process::ExitCode};

use oasreader::{OpenApiContentFormat, read, serialize_document};

fn main() -> ExitCode {
    let mut input = None;
    let mut format = None;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--json" => format = Some(OpenApiContentFormat::Json),
            "--yaml" => format = Some(OpenApiContentFormat::Yaml),
            _ => input = Some(argument),
        }
    }

    let Some(input) = input else {
        eprintln!("usage: merge <path-or-url> [--json | --yaml]");
        return ExitCode::FAILURE;
    };

    let result = match read(&input) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };

    for diagnostic in &result.diagnostics {
        eprintln!("warning: {diagnostic}");
    }

    match serialize_document(&result.document, format.unwrap_or(result.format)) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
