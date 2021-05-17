// #![allow(unused)] // silence unused warnings
mod argv;

use argv::cmd_app;
use fs::canonicalize;
use globwalk::GlobError;
use io::{BufReader, Write};
use libflate::gzip::Encoder;
use serde_json::Value;
use std::{
	fs::{self, copy, create_dir_all, remove_dir_all, remove_file, write, File},
	io,
	path::{Path, PathBuf},
	process::{Command, Stdio},
};
use tar::Builder;
use thiserror::Error;

const AUDIT_ROOT_DIR_NAME: &str = ".audit";

#[derive(Error, Debug)]
pub enum MainError {
	/// Path not safe to delete
	#[error("Path not safe to delete: {0}")]
	PathNotSafeToDelete(String),

	#[error("Cannot delete non node mobule dir")]
	CantDeleteNonNodeMobuleDir,

	/// Represents a failure to read from input.
	#[error("Read error")]
	ReadError { source: std::io::Error },

	#[error("Path Error")]
	PathNotExist(String),

	#[error(transparent)]
	SerdeError(#[from] serde_json::Error),

	#[error(transparent)]
	GlobError(#[from] GlobError),

	/// Represents all other cases of `std::io::Error`.
	#[error(transparent)]
	IOError(#[from] std::io::Error),
}

fn main() {
	// parse the arguments
	let cmd = cmd_app().get_matches();

	// check path validity (must contain package.json)
	let root_str = cmd.value_of("PATH").unwrap_or(".");
	let root = Path::new(root_str);
	let package_path = root.to_path_buf().join("package.json");
	let package_path = package_path.as_path();
	if !package_path.is_file() {
		println!(
			"ERROR - Path '{}' does not contain a package.json - abort",
			package_path.to_str().unwrap()
		);
		return;
	}

	let drop_name = get_drop_name(&root).unwrap();

	let do_clean = cmd.is_present("clean");
	let do_install = !cmd.is_present("no_install");
	let do_audit = !cmd.is_present("no_audit");

	// init the audit dir
	let audit_root_dir = PathBuf::from(root).join(AUDIT_ROOT_DIR_NAME);
	let audit_root_dir = audit_root_dir.as_path();
	let audit_name = format!("{}-AUDIT", drop_name);
	let audit_dir = PathBuf::from(audit_root_dir).join(&audit_name);
	let audit_dir = audit_dir.as_path();

	// clean the audit dir
	if do_audit {
		safer_remove_dir(audit_dir).expect("Canot remove audit dir");
		create_dir_all(audit_dir).expect("Cant create audit dir");
	}

	// list of package.json directories
	let dirs = list_package_dirs(root).unwrap();

	// clean packages
	if do_clean {
		clean_packages(root).expect("Can't clean packages");
	}

	// npm install
	if do_install {
		for (dir, name) in dirs.iter() {
			println!("=== npm install {}\n", name);
			cmd_install(dir.as_path());
		}
	}

	// audit
	if do_audit {
		let mut audit_content = String::new();
		for (dir, name) in dirs.iter() {
			let out = cmd_audit(dir);
			let txt = format!("\n==== AUDIT FOR  {} ===={}\n", name, out);
			println!("{}", txt);
			audit_content.push_str(&txt);
		}

		// write the audit file content
		audit_content.push_str(
			"\n\n========= NOTE:\nnpm audit --audit-level=moderate (for each node directory)\n",
		);

		let audit_file = audit_dir.join("_audit.txt");
		match write(&audit_file, audit_content) {
			Ok(_) => println!("=== Save audit file {}", audit_file.to_str().unwrap()),
			Err(ex) => println!("ERROR {}", ex),
		}

		//// copy the package-locks
		for (dir, name) in dirs.iter() {
			let package_lock_file = dir.join("package-lock.json");
			let name = format!("{}-package-lock.json", name.replace("/", "-"));
			let dist = audit_dir.join(name);
			copy(package_lock_file, dist).expect("Fail to copy a package-lock");
		}
	}

	// create tar & gz
	if do_audit {
		// create tar
		let tar_name = format!("{}.tar", audit_name);
		println!("=== create tar file {}", tar_name);
		let tar_path = audit_root_dir.join(&tar_name);
		let tar_file = File::create(tar_path.as_path()).unwrap();
		let mut a = Builder::new(tar_file);

		for (file, name) in list_audit_files(audit_dir).unwrap().iter() {
			let name = format!("{}/{}", audit_name, name);
			a.append_file(name, &mut File::open(file).unwrap()).unwrap();
		}

		// create gz
		let gz_name = format!("{}.gz", &tar_name);
		println!("=== creating gz file {}", tar_name);
		let tar_file = File::open(tar_path.as_path()).unwrap();
		let mut reader = BufReader::new(tar_file);
		let mut encoder = Encoder::new(Vec::new()).unwrap();
		io::copy(&mut reader, &mut encoder).unwrap();
		let encoded_data = encoder.finish().into_result().unwrap();
		let gz_file = audit_root_dir.join(gz_name);
		let mut gz_file = File::create(gz_file.as_path()).unwrap();
		gz_file.write_all(&encoded_data).expect("Fail to create gz");
	}
}

// region:    package parser
fn get_drop_name(root: &Path) -> Result<String, MainError> {
	//
	let path = root.join("package.json");
	let reader = BufReader::new(File::open(path)?);
	let json: Value = serde_json::from_reader(reader)?;
	let name = match &json["__version__"] {
		serde_json::Value::String(str) => str,
		_ => "DROP-UNKNOWN",
	};
	Ok(name.to_owned())
}
// endregion: package parser

// region:    cmds
fn cmd_install(dir: &Path) {
	let mut proc = Command::new("npm")
		.current_dir(dir)
		.arg("install")
		.arg("--colors")
		.spawn()
		.expect("failed to execute process");

	proc.wait().expect("Fail to wap for npm install");
}

fn cmd_audit(dir: &Path) -> String {
	let output = Command::new("npm")
		.current_dir(dir)
		.arg("audit")
		.arg("--audit-level=moderate")
		.stdout(Stdio::piped())
		.output()
		.expect("failed to execute process");
	let mut output = String::from_utf8(output.stdout).unwrap();

	// Note: need to format and filter since the output seems to have special characters
	// TODO: Probably a simpler way to do this
	output = output.replace("[90m", "");
	output = output.replace("[39m", "");
	// clean each line
	output = output
		.lines()
		.map(|s| {
			s.replace(|c: char| !c.is_alphanumeric() && !c.is_whitespace(), "")
				.trim()
				.to_owned()
		})
		.collect::<Vec<String>>()
		.join("\n");
	output
}

fn clean_packages(root: &Path) -> Result<(), MainError> {
	let dirs = list_package_dirs(root)?;

	for (dir, name) in dirs {
		println!("=== clean {}", name);
		// delete node_modules/
		let node_modules_dir = dir.join("node_modules");
		safer_remove_dir(node_modules_dir.as_path())?;

		// delete package-lock.json
		let package_lock_file = dir.join("package-lock.json");
		safer_remove_file(package_lock_file.as_path())?;
	}

	Ok(())
}
// endregion: cmds

// region:    list files and dirs
fn list_audit_files(root: &Path) -> Result<Vec<(PathBuf, String)>, MainError> {
	let root = canonicalize(root)?;
	let root = root.as_path();

	let walker = globwalk::GlobWalkerBuilder::from_patterns(root.to_str().unwrap(), &["**/*.*"])
		.build()?
		.into_iter()
		.filter_map(Result::ok);

	let v: Vec<(PathBuf, String)> = walker
		.map(|e| {
			let dir_path: PathBuf = e.into_path();
			let rel = dir_path.strip_prefix(root).unwrap();
			let name = rel.to_str().unwrap().to_owned();
			(dir_path, name)
		})
		.collect();
	Ok(v)
}

fn list_package_dirs(root: &Path) -> Result<Vec<(PathBuf, String)>, MainError> {
	let root = canonicalize(root)?;
	let root = root.as_path();

	let walker = globwalk::GlobWalkerBuilder::from_patterns(
		root.to_str().unwrap(),
		&["**/*/package.json", "!**/node_modules/*", "!.git/*"],
	)
	.build()?
	.into_iter()
	.filter_map(Result::ok);

	let mut v: Vec<(PathBuf, String)> = walker
		.map(|e| {
			let dir_path: PathBuf = e.into_path().parent().unwrap().into();
			let rel = dir_path.strip_prefix(root).unwrap();
			let name = rel.to_str().unwrap().to_owned();
			(dir_path, name)
		})
		.collect();

	v.insert(0, (root.to_path_buf().clone(), "_root_".to_owned()));
	Ok(v)
}
// endregion: list files and dirs

// region:    safer_remove funtions
/// Safer remove dir, only allows to remove paths with "node_modules" or ".audit"
fn safer_remove_dir(path: &Path) -> Result<bool, MainError> {
	let path_str = path.to_str().unwrap();
	// safety guard
	if !(path_str.contains("node_modules") || path_str.contains(".audit")) {
		return Err(MainError::PathNotSafeToDelete(path_str.to_owned()));
	}

	if path.is_dir() {
		remove_dir_all(path)?;
		println!("Deleted DIR  - {}", path_str);
		Ok(true)
	} else {
		Ok(false)
	}
}

/// safer remove file, only allow to remove files with "package-lock"
fn safer_remove_file(path: &Path) -> Result<bool, MainError> {
	let path_str = path.to_str().unwrap();
	// safety guard
	if !path_str.contains("package-lock") {
		return Err(MainError::PathNotSafeToDelete(path_str.to_owned()));
	}

	if path.is_file() {
		remove_file(path)?;
		println!("Deleted FILE - {}", path_str);
		Ok(true)
	} else {
		Ok(false)
	}
}
// endregion: safer_remove funtions
