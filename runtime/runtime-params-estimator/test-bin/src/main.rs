use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let build_test_contract = "echo 123";
    let output = std::process::Command::new(build_test_contract)
        .output()
        .with_context(|| format!("failed to run `{}`", build_test_contract))?;
    println!("{}", &String::from_utf8(output.stdout)?);
    println!("{}", &String::from_utf8(output.stderr)?);
    if !output.status.success() {
        anyhow::bail!("failed to run `{}`", build_test_contract);
    }
    let stdout = String::from_utf8(output.stdout)
        .with_context(|| format!("failed to run `{}`", build_test_contract))?;
    println!("{}", stdout.trim().to_string());
    Ok(())
}
