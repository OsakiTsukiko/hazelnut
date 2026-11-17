//! Build/install Hazelnut and register its per-user Linux desktop launcher.
use std::{
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{self, Command},
};

fn main() {
    if let Err(error) = install() {
        eprintln!("Install failed: {error}");
        process::exit(1);
    }
}

fn install() -> Result<(), Box<dyn Error>> {
    let mut desktop_only = false;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--desktop-only" => desktop_only = true,
            "--help" | "-h" => {
                println!(
                    "cargo install-release [--desktop-only]\n\nBuild and install a release binary, terminal command, and Linux desktop launcher.\nUse --desktop-only to integrate an already installed binary without rebuilding."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown option: {argument}").into()),
        }
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME is not set")?);
    let root = env::var_os("CARGO_INSTALL_ROOT")
        .map(PathBuf::from)
        .or_else(|| env::var_os("CARGO_HOME").map(PathBuf::from))
        .unwrap_or_else(|| home.join(".cargo"));
    let root = absolute(&root)?;
    let binary = root
        .join("bin")
        .join(format!("hazelnut{}", env::consts::EXE_SUFFIX));
    if !desktop_only {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("missing project directory")?;
        let status = Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(project)
            .args([
                "install",
                "--path",
                ".",
                "--bin",
                "hazelnut",
                "--profile",
                "release",
                "--locked",
                "--force",
                "--root",
            ])
            .arg(&root)
            .status()?;
        if !status.success() {
            return Err("release build/install failed".into());
        }
    }
    if !binary.is_file() {
        return Err(format!(
            "{} is missing; run cargo install-release to build it",
            binary.display()
        )
        .into());
    }
    println!("Installed binary: {}", binary.display());
    #[cfg(target_os = "linux")]
    integrate_linux(&home, &binary)?;
    Ok(())
}

fn absolute(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        env::current_dir()?.join(path)
    })
}

#[cfg(target_os = "linux")]
fn integrate_linux(home: &Path, binary: &Path) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;
    let bin_dir = home.join(".local/bin");
    let link = bin_dir.join("hazelnut");
    fs::create_dir_all(&bin_dir)?;
    if link != binary {
        match fs::symlink_metadata(&link) {
            Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(&link)?,
            Ok(_) => {
                return Err(format!(
                    "{} already exists and is not a symlink; leaving it intact",
                    link.display()
                )
                .into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        symlink(binary, &link)?;
    }
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let data = absolute(&data)?;
    let icons = data.join("icons/hicolor/scalable/apps");
    fs::create_dir_all(&icons)?;
    fs::write(
        icons.join("hazelnut.svg"),
        include_bytes!("../../assets/icons/hazelnut.svg"),
    )?;
    let png_icons = data.join("icons/hicolor/256x256/apps");
    fs::create_dir_all(&png_icons)?;
    fs::write(
        png_icons.join("hazelnut.png"),
        include_bytes!("../../assets/icons/hazelnut.png"),
    )?;
    let applications = data.join("applications");
    fs::create_dir_all(&applications)?;
    let desktop = applications.join("hazelnut.desktop");
    let exec = desktop_argument(binary)?;
    // An absolute icon path works immediately, including on desktops with a
    // stale or invalid user icon-theme cache.
    let icon = icons
        .join("hazelnut.svg")
        .to_string_lossy()
        .replace('\\', "\\\\");
    fs::write(
        &desktop,
        format!(
            "[Desktop Entry]\nType=Application\nName=Hazelnut\nComment=A small rich-text notebook\nExec={exec} %f\nIcon={icon}\nTerminal=false\nCategories=Office;\nStartupWMClass=hazelnut\nKeywords=notes;notebook;writing;\n"
        ),
    )?;
    // Optional desktop utilities are absent on some Linux distributions.
    if let Ok(status) = Command::new("update-desktop-database")
        .arg(&applications)
        .status()
        && !status.success()
    {
        eprintln!("Desktop cache refresh failed; the launcher was still installed.");
    }
    println!("Terminal command: {}", link.display());
    println!("GNOME launcher: {}", desktop.display());
    let path = env::var_os("PATH").unwrap_or_default();
    if !env::split_paths(&path).any(|entry| entry == bin_dir || entry == binary.parent().unwrap()) {
        println!(
            "Add ~/.local/bin to your shell's PATH, then open a new terminal. You can also run: {}",
            binary.display()
        );
    } else {
        println!("Run hazelnut from your terminal, or search for Hazelnut in GNOME.");
    }
    Ok(())
}

/// Desktop Entry Exec quoting (including both layers of backslash escaping).
#[cfg(target_os = "linux")]
fn desktop_argument(path: &Path) -> Result<String, Box<dyn Error>> {
    let path = path.to_str().ok_or("the installation path must be UTF-8")?;
    if path.contains(['\n', '\r']) {
        return Err("installation paths cannot contain newlines".into());
    }
    let mut result = String::from("\"");
    for c in path.chars() {
        match c {
            '\\' => result.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                result.push_str("\\\\");
                result.push(c);
            }
            '%' => result.push_str("%%"),
            _ => result.push(c),
        }
    }
    result.push('"');
    Ok(result)
}
