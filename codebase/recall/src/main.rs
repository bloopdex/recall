use std::process::ExitCode;

fn main() -> ExitCode {
    match recall::cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            match err.downcast_ref::<recall::Error>() {
                Some(recall_err) => recall::ui::print_error(recall_err),
                None => eprintln!("error: {err}"),
            }
            ExitCode::FAILURE
        }
    }
}
