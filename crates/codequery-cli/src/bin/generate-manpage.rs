use clap::CommandFactory;
use clap_mangen::Man;
use codequery_cli::args::CqArgs;

fn main() {
    let cmd = CqArgs::command();
    let man = Man::new(cmd);

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let path = &args[1];
        let mut file =
            std::fs::File::create(path).unwrap_or_else(|e| panic!("cannot create {path}: {e}"));
        man.render(&mut file)
            .unwrap_or_else(|e| panic!("failed to write man page: {e}"));
        eprintln!("wrote {path}");
    } else {
        man.render(&mut std::io::stdout())
            .expect("failed to write man page to stdout");
    }
}
