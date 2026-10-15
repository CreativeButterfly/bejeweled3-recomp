use bej3pak::{write_pak, Pak};
use std::collections::HashSet;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:
  bej3pak info <main.pak>              summary of the archive
  bej3pak list <main.pak>              size and path of every entry
  bej3pak cat  <main.pak> <path>       write one entry to stdout (redirect it)
  bej3pak pack <dir> <out.pak>         build a PAK from a folder (needs <dir>/PROVENANCE.txt)"
    );
    ExitCode::from(2)
}

/// Collect every file under `dir` as (path relative to `base` with forward slashes, full path).
fn collect(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect(&path, base, out)?;
        } else {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
    Ok(())
}

/// Build a PAK from a folder. Every file must be listed in PROVENANCE.txt, one
/// line per file: `relative/path | where it came from | license`.
fn pack(dir: &str, out_path: &str) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let base = Path::new(dir);

    let mut found = Vec::new();
    collect(base, base, &mut found)?;
    found.retain(|(rel, _)| {
        let l = rel.to_ascii_lowercase();
        l != "provenance.txt" && !l.ends_with("thumbs.db") && !l.ends_with(".ds_store")
    });
    found.sort_by(|a, b| a.0.cmp(&b.0));

    let prov = std::fs::read_to_string(base.join("PROVENANCE.txt"))
        .map_err(|e| format!("cannot read {dir}/PROVENANCE.txt: {e}"))?;
    let listed: HashSet<String> = prov
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split('|').next())
        .map(|p| p.trim().replace('\\', "/").to_ascii_lowercase())
        .collect();

    let missing: Vec<&str> = found
        .iter()
        .filter(|(rel, _)| !listed.contains(&rel.to_ascii_lowercase()))
        .map(|(rel, _)| rel.as_str())
        .collect();
    if !missing.is_empty() {
        eprintln!("refusing to pack: these files have no PROVENANCE.txt entry:");
        for m in missing {
            eprintln!("  {m}");
        }
        return Ok(ExitCode::FAILURE);
    }

    let mut files = Vec::with_capacity(found.len());
    for (rel, path) in &found {
        files.push((rel.clone(), std::fs::read(path)?));
    }

    let file = std::fs::File::create(out_path)?;
    let mut w = io::BufWriter::new(file);
    write_pak(&mut w, &files)?;
    w.flush()?;
    println!("wrote {} entries to {}", files.len(), out_path);
    Ok(ExitCode::SUCCESS)
}

fn run(args: &[String]) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["info", path] => {
            let pak = Pak::open(path)?;
            let total: u64 = pak.entries().iter().map(|e| e.size as u64).sum();
            println!("entries:     {}", pak.entries().len());
            println!("data start:  {}", pak.data_start());
            println!("data bytes:  {}", total);
            println!("file bytes:  {}", pak.file_len());
            Ok(ExitCode::SUCCESS)
        }
        ["list", path] => {
            let pak = Pak::open(path)?;
            let stdout = io::stdout();
            let mut out = io::BufWriter::new(stdout.lock());
            for e in pak.entries() {
                writeln!(out, "{:>10}  {}", e.size, e.name)?;
            }
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
        ["cat", path, entry] => {
            // Entries are mostly binary; don't spray them across a terminal.
            if io::stdout().is_terminal() {
                eprintln!("refusing to write binary data to a terminal; redirect stdout");
                return Ok(ExitCode::from(2));
            }
            let pak = Pak::open(path)?;
            let data = pak.read(entry)?;
            let mut out = io::stdout().lock();
            out.write_all(&data)?;
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
        ["pack", dir, out] => pack(dir, out),
        _ => Ok(usage()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
