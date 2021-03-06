// Copyright 2020 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    anyhow::{Context as _, Error},
    fidl_fuchsia_test_manager as ftest_manager,
    ftest_manager::{CaseStatus, RunOptions, SuiteStatus},
    fuchsia_async as fasync,
    pretty_assertions::assert_eq,
    test_manager_test_lib::RunEvent,
};

fn default_options() -> RunOptions {
    RunOptions { ..RunOptions::EMPTY }
}

pub async fn run_test(test_url: &str, run_options: RunOptions) -> Result<Vec<RunEvent>, Error> {
    let run_builder = test_runners_test_lib::connect_to_test_manager().await?;
    let builder = test_manager_test_lib::TestBuilder::new(run_builder);
    let suite_instance =
        builder.add_suite(test_url, run_options).await.context("Cannot create suite instance")?;
    let builder_run = fasync::Task::spawn(async move { builder.run().await });
    let (events, _logs) = test_runners_test_lib::process_events(suite_instance, false).await?;
    builder_run.await.context("builder execution failed")?;
    Ok(events)
}

#[fuchsia_async::run_singlethreaded(test)]
async fn launch_and_run_passing_test() {
    let test_url = "fuchsia-pkg://fuchsia.com/elf-test-runner-example-tests#meta/passing_test.cm";

    let events = run_test(test_url, default_options()).await.unwrap();

    let expected_events = vec![
        RunEvent::case_found("main"),
        RunEvent::case_started("main"),
        RunEvent::case_stopped("main", CaseStatus::Passed),
        RunEvent::case_finished("main"),
        RunEvent::suite_finished(SuiteStatus::Passed),
    ];
    assert_eq!(events, expected_events);
}

#[fuchsia_async::run_singlethreaded(test)]
async fn launch_and_run_failing_test() {
    let test_url = "fuchsia-pkg://fuchsia.com/elf-test-runner-example-tests#meta/failing_test.cm";

    let events = run_test(test_url, default_options()).await.unwrap();

    let expected_events = vec![
        RunEvent::case_found("main"),
        RunEvent::case_started("main"),
        RunEvent::case_stopped("main", CaseStatus::Failed),
        RunEvent::case_finished("main"),
        RunEvent::suite_finished(SuiteStatus::Failed),
    ];
    assert_eq!(events, expected_events);
}

#[fuchsia_async::run_singlethreaded(test)]
async fn launch_and_run_test_with_custom_args() {
    let test_url = "fuchsia-pkg://fuchsia.com/elf-test-runner-example-tests#meta/arg_test.cm";
    let mut options = default_options();
    options.arguments = Some(vec!["expected_arg".to_owned()]);
    let events = run_test(test_url, options).await.unwrap();

    let expected_events = vec![
        RunEvent::case_found("main"),
        RunEvent::case_started("main"),
        RunEvent::case_stdout("main", "Got argv[1]=\"expected_arg\""),
        RunEvent::case_stopped("main", CaseStatus::Passed),
        RunEvent::case_finished("main"),
        RunEvent::suite_finished(SuiteStatus::Passed),
    ];
    assert_eq!(expected_events, events);
}
