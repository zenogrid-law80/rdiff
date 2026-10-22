mod assets;
mod cli;
mod diff;
mod folding;
mod i18n;
mod merge;
mod merge_ui;
mod settings;
mod surface;
mod syntax;
mod ui;
mod update;
mod update_ui;

fn main() -> std::process::ExitCode {
    match cli::parse(std::env::args_os().skip(1).collect()) {
        Ok(cli::Command::Update { check }) => match update::run(check) {
            Ok(message) => {
                println!("{message}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("Update failed: {error}");
                std::process::ExitCode::from(2)
            }
        },
        Ok(cli::Command::Diff(paths)) => {
            ui::run(paths);
            std::process::ExitCode::SUCCESS
        }
        Ok(cli::Command::Merge(paths)) => std::process::ExitCode::from(merge_ui::run(paths)),
        Ok(cli::Command::Help) => {
            println!("{}", cli::USAGE);
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}\n\n{}", cli::USAGE);
            std::process::ExitCode::from(2)
        }
    }
}

// Exercise the exact vendored overlay implementation in the application test
// suite without requiring the vendor's optional platform test dependencies.
#[cfg(test)]
mod decoration_regression {
    use gpui_kit as gpui;
    include!("../vendor/gpui-base/src/input/base/decoration_composition.rs");
}
