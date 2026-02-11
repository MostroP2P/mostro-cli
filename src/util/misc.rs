use std::{fs, path::Path};

pub fn uppercase_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

pub fn get_mcli_path() -> String {
    let home_dir = dirs::home_dir().expect("Couldn't get home directory");
    let mcli_path = format!("{}/.mcliUserB", home_dir.display());
    if !Path::new(&mcli_path).exists() {
        match fs::create_dir(&mcli_path) {
            Ok(_) => println!("Directory {} created.", mcli_path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Directory was created by another thread/process, which is fine
            }
            Err(e) => panic!("Couldn't create mostro-cli directory in HOME: {}", e),
        }
    }
    mcli_path
}
