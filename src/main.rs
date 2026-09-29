use rusty_cleaner::{applications, scan, scanner::Feature};
use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") || args.is_empty() {
        print_help();
        return;
    }

    if args.first().map(String::as_str) == Some("apps") {
        list_apps();
        return;
    }

    if args.first().map(String::as_str) != Some("scan") {
        eprintln!("Comando desconhecido. Use `rusty-cleaner --help`.");
        std::process::exit(2);
    }

    let selected = args.windows(2).find(|pair| pair[0] == "--feature");
    let feature = match selected {
        Some(pair) => match Feature::parse(&pair[1]) {
            Some(feature) => Some(feature),
            None => {
                eprintln!("Feature inválida: {}", pair[1]);
                print_help();
                std::process::exit(2);
            }
        },
        None => None,
    };

    let findings = scan(feature);
    let total_bytes: u64 = findings.iter().map(|item| item.size).sum();
    println!(
        "{} candidato(s), {:.2} MiB no total",
        findings.len(),
        total_bytes as f64 / 1_048_576.0
    );
    for item in findings {
        println!(
            "[{}] {} | {} bytes | {}",
            item.feature,
            item.name,
            item.size,
            item.path.display()
        );
    }
}

fn print_help() {
    println!("CleanOS Pro - scanner somente leitura\n\nUso:\n  rusty-cleaner scan [--feature NOME]\n  rusty-cleaner apps\n\nFeatures: orphan, temp, chat-media, trash, browser, duplicates, large-old, registry\nO recurso registry exige Windows. Nenhum arquivo e apagado por este prototipo.");
}

fn list_apps() {
    let apps = applications::scan();
    let mut apps = apps;
    apps.sort_by_key(|app| app.last_used);
    println!("{} aplicativo(s)", apps.len());
    for app in apps {
        let days = app
            .last_used
            .and_then(|time| time.elapsed().ok())
            .map(|age| age.as_secs() / 86_400)
            .map_or("?".to_owned(), |days| format!("{days}d"));
        let size = app.size.map_or("? MiB".to_owned(), |bytes| {
            format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
        });
        let uninstall = app
            .uninstall
            .as_ref()
            .map(|action| format!("{}:{}", action.kind(), crate::app_arg(action)))
            .unwrap_or_else(|| "none".to_owned());
        println!(
            "[{uninstall}] {days} | {size} | {} | {}",
            app.name,
            app.path.display()
        );
    }
}

fn app_arg(action: &applications::Uninstall) -> String {
    match action {
        applications::Uninstall::Pacman(arg)
        | applications::Uninstall::Dpkg(arg)
        | applications::Uninstall::Rpm(arg)
        | applications::Uninstall::Flatpak(arg)
        | applications::Uninstall::Snap(arg)
        | applications::Uninstall::Windows(arg) => arg.clone(),
        applications::Uninstall::MacBundle(path) => path.display().to_string(),
    }
}
