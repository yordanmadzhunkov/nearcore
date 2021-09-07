use anyhow::Context;
use std::path::PathBuf;

fn project_root() -> PathBuf {
    let dir = env!("CARGO_MANIFEST_DIR");
    let res = PathBuf::from(dir).ancestors().nth(3).unwrap().to_owned();
    assert!(res.join(".github").exists());
    res
}

fn main() -> anyhow::Result<()> {
    let build_test_contract = "set -ex && ./build.sh";
    let project_root = project_root();
    let estimator_dir = project_root.join("runtime/runtime-params-estimator/test-contract");
    let output = std::process::Command::new(build_test_contract)
        .current_dir(estimator_dir)
        .output()
        .unwrap();
    let out = String::from_utf8(output.stdout.clone())?;
    let err = String::from_utf8(output.stderr.clone())?;
    println!("{}", &out);
    println!("{}", &err);
    if !output.status.success() {
        anyhow::bail!("failed to run `{}`", build_test_contract);
    }
    let stdout = String::from_utf8(output.stdout)
        .with_context(|| format!("failed to run `{}`", build_test_contract))?;
    println!("{}", stdout.trim().to_string());
    Ok(())
}
