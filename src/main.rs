mod cli;
mod diff;
mod folding;
mod merge;
mod merge_ui;
mod surface;
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
