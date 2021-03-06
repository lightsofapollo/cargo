use std::os::args;
use std::io;
use std::path::Path;
use std::str;
use core::{Package,PackageSet,Target};
use util;
use util::{other_error,human_error,CargoResult,CargoError,ProcessBuilder};
use util::result::ProcessError;

type Args = Vec<String>;

pub fn compile_packages(pkg: &Package, deps: &PackageSet) -> CargoResult<()> {
    debug!("compile_packages; pkg={}; deps={}", pkg, deps);

    let target_dir = pkg.get_absolute_target_dir();
    let deps_target_dir = target_dir.join("deps");

    // First ensure that the destination directory exists
    debug!("creating target dir; path={}", target_dir.display());
    try!(mk_target(&target_dir));
    try!(mk_target(&deps_target_dir));

    // Traverse the dependencies in topological order
    for dep in try!(topsort(deps)).iter() {
        println!("Compiling {}", dep);
        try!(compile_pkg(dep, &deps_target_dir, &deps_target_dir, false));
    }

    println!("Compiling {}", pkg);
    try!(compile_pkg(pkg, &target_dir, &deps_target_dir, true));

    Ok(())
}

fn compile_pkg(pkg: &Package, dest: &Path, deps_dir: &Path, primary: bool) -> CargoResult<()> {
    debug!("compile_pkg; pkg={}; targets={}", pkg, pkg.get_targets());

    // compile
    for target in pkg.get_targets().iter() {
        // Only compile lib targets for dependencies
        if primary || target.is_lib() {
            try!(rustc(&pkg.get_root(), target, dest, deps_dir, primary))
        }
    }

    Ok(())
}

fn mk_target(target: &Path) -> CargoResult<()> {
    io::fs::mkdir_recursive(target, io::UserRWX)
      .map_err(|_| other_error("could not create target directory"))
}

fn rustc(root: &Path, target: &Target, dest: &Path, deps: &Path, verbose: bool) -> CargoResult<()> {

    let crate_types = target.rustc_crate_types();

    for crate_type in crate_types.iter() {
        log!(5, "root={}; target={}; crate_type={}; dest={}; deps={}; verbose={}",
                root.display(), target, crate_type, dest.display(), deps.display(), verbose);

        let rustc = prepare_rustc(root, target, *crate_type, dest, deps);

        try!((if verbose {
            rustc.exec()
        } else {
            rustc.exec_with_output().and(Ok(()))
        }).map_err(|e| rustc_to_cargo_err(rustc.get_args().as_slice(), root, e)));
    }

    Ok(())
}

fn prepare_rustc(root: &Path, target: &Target, crate_type: &'static str, dest: &Path, deps: &Path) -> ProcessBuilder {
    let mut args = Vec::new();

    build_base_args(&mut args, target, crate_type, dest);
    build_deps_args(&mut args, deps);

    util::process("rustc")
        .cwd(root.clone())
        .args(args.as_slice())
        .env("RUST_LOG", None) // rustc is way too noisy
}

fn build_base_args(into: &mut Args, target: &Target, crate_type: &'static str, dest: &Path) {
    // TODO: Handle errors in converting paths into args
    into.push(target.get_path().display().to_str());
    into.push("--crate-type".to_str());
    into.push(crate_type.to_str());
    into.push("--out-dir".to_str());
    into.push(dest.display().to_str());
}

fn build_deps_args(dst: &mut Args, deps: &Path) {
    dst.push("-L".to_str());
    dst.push(deps.display().to_str());
}

fn rustc_to_cargo_err(args: &[String], cwd: &Path, err: CargoError) -> CargoError {
    let msg = {
        let output = match err {
            CargoError { kind: ProcessError(_, ref output), .. } => output,
            _ => fail!("Bug! exec() returned an error other than a ProcessError")
        };

        let mut msg = format!("failed to execute: `rustc {}`", args.connect(" "));

        output.as_ref().map(|o| {
            let second = format!("; Error:\n{}", str::from_utf8_lossy(o.error.as_slice()));
            msg.push_str(second.as_slice());
        });

        msg
    };

    human_error(msg, format!("root={}", cwd.display()), err)
}

fn topsort(deps: &PackageSet) -> CargoResult<PackageSet> {
    match deps.sort() {
        Some(deps) => Ok(deps),
        None => return Err(other_error("circular dependency detected"))
    }
}
