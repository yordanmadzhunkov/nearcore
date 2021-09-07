use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let build_test_contract = "echo";
    let output = std::process::Command::new(build_test_contract).args(["123"]).output().unwrap();
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
