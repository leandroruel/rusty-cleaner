use rusty_cleaner::{scan, scanner::Feature};
use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") || args.is_empty() {
        print_help();
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
    println!("CleanOS Pro - scanner somente leitura\n\nUso:\n  rusty-cleaner scan [--feature NOME]\n\nFeatures: orphan, temp, chat-media, trash, browser, duplicates, large-old, registry\nO recurso registry exige Windows. Nenhum arquivo e apagado por este prototipo.");
}
