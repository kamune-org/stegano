use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use stegano_core::{EmbedOptions, Entry, EntryKind};

/// Hide encrypted text and files inside images and audio.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show a carrier's format and how much it can hold.
    Info { carrier: PathBuf },

    /// Hide a message and/or files in a carrier.
    Hide {
        carrier: PathBuf,
        /// Where to write the result (same format as the carrier).
        #[arg(short, long)]
        output: PathBuf,
        /// A text message to hide.
        #[arg(short, long)]
        message: Option<String>,
        /// A file to hide (repeatable).
        #[arg(short, long = "file")]
        files: Vec<PathBuf>,
        /// Don't compress the payload.
        #[arg(long)]
        no_compress: bool,
        /// Passphrase (prompted for if not given).
        #[arg(long, env = "STEGANO_PASSPHRASE", hide_env_values = true)]
        passphrase: Option<String>,
    },

    /// Reveal what is hidden in a carrier.
    Reveal {
        carrier: PathBuf,
        /// Directory to write hidden files into.
        #[arg(short = 'd', long, default_value = ".")]
        out_dir: PathBuf,
        /// Overwrite existing files.
        #[arg(long)]
        force: bool,
        /// Passphrase (prompted for if not given).
        #[arg(long, env = "STEGANO_PASSPHRASE", hide_env_values = true)]
        passphrase: Option<String>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Info { carrier } => {
            let info = stegano_core::inspect(&read(&carrier)?)?;
            println!("{} ({})", info.format.name(), info.description);
            println!("capacity:    {}", bytes(info.capacity));
            println!(
                "recommended: {} (keeps detection risk low)",
                bytes(info.recommended)
            );
        }

        Command::Hide {
            carrier,
            output,
            message,
            files,
            no_compress,
            passphrase,
        } => {
            let mut entries = Vec::new();
            if let Some(text) = message {
                entries.push(Entry::text(text));
            }
            for path in &files {
                let name = path
                    .file_name()
                    .context("file has no name")?
                    .to_string_lossy()
                    .into_owned();
                let modified = fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                entries.push(Entry::file(name, read(path)?, modified));
            }
            if entries.is_empty() {
                bail!("nothing to hide: pass --message and/or --file");
            }
            let passphrase = passphrase_or_prompt(passphrase, true)?;
            let options = EmbedOptions {
                compress: !no_compress,
            };
            let result = stegano_core::embed(&read(&carrier)?, &entries, &passphrase, &options)?;
            let r = &result.report;
            fs::write(&output, &result.data)
                .with_context(|| format!("writing {}", output.display()))?;
            println!(
                "hid {} in {} ({} of {} elements changed)",
                bytes(r.embedded_bytes),
                output.display(),
                r.changed,
                r.elements
            );
            if r.embedded_bytes > r.recommended {
                eprintln!(
                    "warning: payload exceeds the recommended {} for this carrier; \
                     it is more likely to be detected",
                    bytes(r.recommended)
                );
            }
            if output.extension().and_then(|e| e.to_str()) != Some(result.format.extension()) {
                eprintln!(
                    "note: output is {}; consider a .{} extension",
                    result.format.name(),
                    result.format.extension()
                );
            }
        }

        Command::Reveal {
            carrier,
            out_dir,
            force,
            passphrase,
        } => {
            let passphrase = passphrase_or_prompt(passphrase, false)?;
            let entries = stegano_core::extract(&read(&carrier)?, &passphrase)?;
            for entry in entries {
                match entry.kind {
                    EntryKind::Text => println!("{}", entry.as_text()),
                    EntryKind::File => {
                        let path = out_dir.join(entry.safe_file_name());
                        if path.exists() && !force {
                            bail!("{} exists (use --force to overwrite)", path.display());
                        }
                        fs::write(&path, &entry.data)
                            .with_context(|| format!("writing {}", path.display()))?;
                        eprintln!("wrote {} ({})", path.display(), bytes(entry.data.len()));
                    }
                }
            }
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("reading {}", path.display()))
}

fn passphrase_or_prompt(given: Option<String>, confirm: bool) -> Result<String> {
    if let Some(p) = given {
        return Ok(p);
    }
    if !std::io::stdin().is_terminal() {
        bail!("no passphrase: pass --passphrase or set STEGANO_PASSPHRASE");
    }
    let p = rpassword::prompt_password("Passphrase: ")?;
    if p.is_empty() {
        bail!("passphrase must not be empty");
    }
    if confirm && rpassword::prompt_password("Repeat passphrase: ")? != p {
        bail!("passphrases don't match");
    }
    Ok(p)
}

fn bytes(n: usize) -> String {
    match n {
        0..1024 => format!("{n} B"),
        1024..1_048_576 => format!("{:.1} KiB", n as f64 / 1024.0),
        _ => format!("{:.1} MiB", n as f64 / 1_048_576.0),
    }
}
