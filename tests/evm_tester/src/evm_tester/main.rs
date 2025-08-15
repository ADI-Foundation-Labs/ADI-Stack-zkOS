//!
//! The evm tester executable.
//!

pub(crate) mod arguments;

use std::time::Instant;

use colored::Colorize;

use self::arguments::Arguments;

/// The rayon worker stack size.
const RAYON_WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

///
/// The application entry point.
///
fn main() {
    let exit_code = match main_inner(Arguments::new()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error:?}");
            1
        }
    };
    std::process::exit(exit_code);
}

///
/// The entry point wrapper used for proper error handling.
///
fn main_inner(arguments: Arguments) -> anyhow::Result<()> {
    let mut thread_pool_builder = rayon::ThreadPoolBuilder::new();
    if let Some(threads) = arguments.threads {
        thread_pool_builder = thread_pool_builder.num_threads(threads);
    }
    thread_pool_builder
        .stack_size(RAYON_WORKER_STACK_SIZE)
        .build_global()
        .expect("Thread pool configuration failure");

    let summary = evm_tester::Summary::new(arguments.verbosity, arguments.quiet).wrap();

    let filters = evm_tester::Filters::new(
        arguments.paths,
        arguments.groups,
        arguments.labels,
        arguments.names,
    );

    let evm_tester = evm_tester::EvmTester::new(
        summary.clone(),
        filters,
        arguments.workflow,
        arguments.mutation_path,
        arguments.run_ethereum_spec_tests,
    )?;

    let environment = match arguments.environment {
        Some(environment @ evm_tester::Environment::ZKsyncOS) => environment,
        None => evm_tester::Environment::ZKsyncOS,
    };

    let run_time_start = Instant::now();
    println!(
        "     {} tests with {} worker threads",
        "Running".bright_green().bold(),
        rayon::current_num_threads(),
    );

    match environment {
        evm_tester::Environment::ZKsyncOS => {
            // let vm = evm_tester::ZKsyncOS::new();
            // evm_tester.run_zksync_os(vm, arguments.mutation)

            let vm = evm_tester::vm::zk_os::ethereum_stf::ZKsyncOSEthereumSTF::new();
            evm_tester.run_zksync_os_ethereum_stf(vm, arguments.mutation)
        }
    }?;

    let summary = evm_tester::Summary::unwrap_arc(summary);
    print!("{summary}");
    println!(
        "    {} running tests in {}m{:02}s",
        "Finished".bright_green().bold(),
        run_time_start.elapsed().as_secs() / 60,
        run_time_start.elapsed().as_secs() % 60,
    );

    if !summary.is_successful() {
        anyhow::bail!("");
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::arguments::Arguments;

    #[test]
    fn test_manually() {
        // std::env::set_current_dir("..").expect("Change directory failed");

        let arguments = Arguments {
            verbosity: false,
            quiet: false,
            // paths: vec!["ethereum-fixtures/state_tests".to_owned()],
            paths: vec![],
            // names: vec![],
            names: vec!["precompsEIP2929Cancun".to_string()],
            groups: vec![],
            labels: vec![],
            threads: Some(1),
            environment: None,
            workflow: evm_tester::Workflow::BuildAndRun,
            mutation: false,
            mutation_path: None,
            run_ethereum_spec_tests: true,
        };

        crate::main_inner(arguments).expect("Manual testing failed");
    }
}
