// Copyright 2020 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    anyhow::{format_err, Context, Error},
    fidl::endpoints,
    fidl::endpoints::ClientEnd,
    fidl::endpoints::Proxy,
    fidl_fuchsia_component_runner as fcrunner,
    fidl_fuchsia_io::DirectoryMarker,
    fidl_fuchsia_sys2 as fsys,
    fidl_fuchsia_test::{
        CaseListenerRequest::Finished,
        Invocation, Result_ as TestResult,
        RunListenerRequest::{OnFinished, OnTestCaseStarted},
        RunListenerRequestStream,
    },
    fidl_fuchsia_test_manager as ftest_manager, fuchsia_async as fasync,
    fuchsia_component::client::{self, connect_to_protocol_at_dir_root},
    fuchsia_runtime::job_default,
    fuchsia_zircon as zx,
    futures::channel::mpsc,
    futures::prelude::*,
    runner::component::ComponentNamespace,
    runner::component::ComponentNamespaceError,
    std::collections::HashMap,
    std::convert::TryFrom,
    std::sync::Arc,
    test_manager_test_lib::RunEvent,
    test_runners_lib::elf::{BuilderArgs, Component},
};

#[derive(PartialEq, Debug)]
pub enum ListenerEvent {
    StartTest(String),
    FinishTest(String, TestResult),
    FinishAllTests,
}

fn get_ord_index_and_name(event: &ListenerEvent) -> (usize, &str) {
    match event {
        ListenerEvent::StartTest(name) => (0, name),
        ListenerEvent::FinishTest(name, _) => (1, name),
        ListenerEvent::FinishAllTests => (2, ""),
    }
}

// Orders by test name and then event type.
impl Ord for ListenerEvent {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let (s_index, s_test_name) = get_ord_index_and_name(self);
        let (o_index, o_test_name) = get_ord_index_and_name(other);
        if s_test_name == o_test_name || s_index == 2 || o_index == 2 {
            return s_index.cmp(&o_index);
        }
        return s_test_name.cmp(&o_test_name);
    }
}

// Makes sure that FinishTest event never shows up before StartTest and FinishAllTests is always
// last.
pub fn assert_event_ord(events: &Vec<ListenerEvent>) {
    let mut tests = HashMap::new();
    let mut all_finish = false;
    for event in events {
        assert!(!all_finish, "got FinishAllTests event twice: {:#?}", events);
        match event {
            ListenerEvent::StartTest(name) => {
                assert!(
                    !tests.contains_key(&name),
                    "Multiple StartTest for test {}: {:#?}",
                    name,
                    events
                );
                tests.insert(name, false);
            }
            ListenerEvent::FinishTest(name, _) => {
                assert!(
                    tests.contains_key(&name),
                    "Got finish before start event for test {}: {:#?}",
                    name,
                    events
                );
                assert!(
                    !tests.insert(name, true).unwrap(),
                    "Multiple FinishTest for test {}: {:#?}",
                    name,
                    events
                );
            }
            ListenerEvent::FinishAllTests => {
                all_finish = true;
            }
        }
    }
}

impl PartialOrd for ListenerEvent {
    fn partial_cmp(&self, other: &ListenerEvent) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for ListenerEvent {}

impl ListenerEvent {
    pub fn start_test(name: &str) -> ListenerEvent {
        ListenerEvent::StartTest(name.to_string())
    }
    pub fn finish_test(name: &str, test_result: TestResult) -> ListenerEvent {
        ListenerEvent::FinishTest(name.to_string(), test_result)
    }
    pub fn finish_all_test() -> ListenerEvent {
        ListenerEvent::FinishAllTests
    }
}

impl Clone for ListenerEvent {
    fn clone(&self) -> Self {
        match self {
            ListenerEvent::StartTest(name) => ListenerEvent::start_test(name),
            ListenerEvent::FinishTest(name, test_result) => ListenerEvent::finish_test(
                name,
                TestResult { status: test_result.status.clone(), ..TestResult::EMPTY },
            ),
            ListenerEvent::FinishAllTests => ListenerEvent::finish_all_test(),
        }
    }
}

/// Collects all the listener event as they come and return in a vector.
pub async fn collect_listener_event(
    mut listener: RunListenerRequestStream,
) -> Result<Vec<ListenerEvent>, Error> {
    let mut ret = vec![];
    // collect loggers so that they do not die.
    let mut loggers = vec![];
    while let Some(result_event) = listener.try_next().await? {
        match result_event {
            OnTestCaseStarted { invocation, primary_log, listener, .. } => {
                let name = invocation.name.unwrap();
                ret.push(ListenerEvent::StartTest(name.clone()));
                loggers.push(primary_log);
                let mut listener = listener.into_stream()?;
                while let Some(result) = listener.try_next().await? {
                    match result {
                        Finished { result, .. } => {
                            ret.push(ListenerEvent::FinishTest(name, result));
                            break;
                        }
                    }
                }
            }
            OnFinished { .. } => {
                ret.push(ListenerEvent::FinishAllTests);
                break;
            }
        }
    }
    Ok(ret)
}

/// Helper method to convert names to `Invocation`.
pub fn names_to_invocation(names: Vec<&str>) -> Vec<Invocation> {
    names
        .iter()
        .map(|s| Invocation { name: Some(s.to_string()), tag: None, ..Invocation::EMPTY })
        .collect()
}

// process events by parsing and normalizing logs. Returns `RunEvents` and collected logs.
pub async fn process_events(
    suite_instance: test_manager_test_lib::SuiteRunInstance,
    exclude_empty_logs: bool,
) -> Result<(Vec<RunEvent>, Vec<String>), Error> {
    let (sender, mut recv) = mpsc::channel(1);
    let execution_task =
        fasync::Task::spawn(async move { suite_instance.collect_events(sender).await });
    let mut events = vec![];
    let mut log_tasks = vec![];
    let mut buffered_logs = HashMap::new();
    while let Some(event) = recv.next().await {
        match event.payload {
            test_manager_test_lib::SuiteEventPayload::RunEvent(RunEvent::CaseStdout {
                name,
                stdout_message,
            }) => {
                let logs = stdout_message.split("\n");
                let mut logs = logs.collect::<Vec<&str>>();
                // discard last empty log(if it ended in newline, or  store im-complete line)
                let mut last_incomplete_line = logs.pop();
                if stdout_message.as_bytes().last() == Some(&b'\n') {
                    last_incomplete_line = None;
                }
                for log in logs {
                    if exclude_empty_logs && log.len() == 0 {
                        continue;
                    }
                    let mut msg = log.to_owned();
                    // This is only executed for first log line and used to concat previous
                    // buffered line.
                    if let Some(prev_log) = buffered_logs.remove(&name) {
                        msg = format!("{}{}", prev_log, msg);
                    }
                    events.push(RunEvent::case_stdout(name.clone(), msg));
                    if let Some(log) = last_incomplete_line {
                        let mut log = log.to_owned();
                        if let Some(prev_log) = buffered_logs.remove(&name) {
                            log = format!("{}{}", prev_log, log);
                        }
                        buffered_logs.insert(name.clone(), log);
                    }
                }
            }
            test_manager_test_lib::SuiteEventPayload::RunEvent(e) => events.push(e),
            test_manager_test_lib::SuiteEventPayload::SuiteLog { log_stream } => {
                let t = fasync::Task::spawn(log_stream.collect::<Vec<_>>());
                log_tasks.push(t);
            }
            test_manager_test_lib::SuiteEventPayload::TestCaseLog { .. } => {
                panic!("not supported yet!")
            }
        }
    }
    execution_task.await.context("test execution failed")?;

    for (name, log) in buffered_logs {
        events.push(RunEvent::case_stdout(name, log));
    }

    let mut collected_logs = vec![];
    for t in log_tasks {
        let logs = t.await;
        for log_result in logs {
            let log = log_result?;
            collected_logs.push(log.msg().unwrap().to_string());
        }
    }

    Ok((events, collected_logs))
}

// Binds to test manager component and returns run builder service.
pub async fn connect_to_test_manager() -> Result<ftest_manager::RunBuilderProxy, Error> {
    let realm = client::connect_to_protocol::<fsys::RealmMarker>()
        .context("could not connect to Realm service")?;

    let mut child_ref = fsys::ChildRef { name: "test_manager".to_owned(), collection: None };
    let (dir, server_end) = endpoints::create_proxy::<DirectoryMarker>()?;
    realm
        .bind_child(&mut child_ref, server_end)
        .await
        .context("bind_child fidl call failed for test manager")?
        .map_err(|e| format_err!("failed to create test manager: {:?}", e))?;

    connect_to_protocol_at_dir_root::<ftest_manager::RunBuilderMarker>(&dir)
        .context("failed to open test suite service")
}

fn create_ns_from_current_ns(
    dir_paths: Vec<(&str, u32)>,
) -> Result<ComponentNamespace, ComponentNamespaceError> {
    let mut ns = vec![];
    for (path, permission) in dir_paths {
        let chan = io_util::open_directory_in_namespace(path, permission)
            .unwrap()
            .into_channel()
            .unwrap()
            .into_zx_channel();
        let handle = ClientEnd::new(chan);

        ns.push(fcrunner::ComponentNamespaceEntry {
            path: Some(path.to_string()),
            directory: Some(handle),
            ..fcrunner::ComponentNamespaceEntry::EMPTY
        });
    }
    ComponentNamespace::try_from(ns)
}

/// Create a new component object for testing purposes.
pub async fn test_component(
    url: &str,
    name: &str,
    binary: &str,
    args: Vec<String>,
) -> Result<Arc<Component>, Error> {
    let ns = create_ns_from_current_ns(vec![("/pkg", io_util::OPEN_RIGHT_READABLE)])?;
    let component = Component::create_for_tests(BuilderArgs {
        url: url.to_string(),
        name: name.to_string(),
        binary: binary.to_string(),
        args,
        ns,
        job: job_default().duplicate(zx::Rights::SAME_RIGHTS)?,
    })
    .await?;
    Ok(Arc::new(component))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fidl_fuchsia_test::Status;

    #[test]
    fn test_ordering_by_enum() {
        let expected_events = vec![
            ListenerEvent::start_test("a"),
            ListenerEvent::finish_test(
                "a",
                TestResult { status: Some(Status::Passed), ..TestResult::EMPTY },
            ),
            ListenerEvent::finish_all_test(),
        ];

        let mut events = expected_events.clone();
        events.reverse();

        assert_ne!(events, expected_events);
        events.sort();
        assert_eq!(events, expected_events);
    }

    #[test]
    fn test_ordering_by_test_name() {
        let mut events = vec![
            ListenerEvent::start_test("b"),
            ListenerEvent::start_test("a"),
            ListenerEvent::finish_test(
                "a",
                TestResult { status: Some(Status::Passed), ..TestResult::EMPTY },
            ),
            ListenerEvent::start_test("c"),
            ListenerEvent::finish_test(
                "b",
                TestResult { status: Some(Status::Passed), ..TestResult::EMPTY },
            ),
            ListenerEvent::finish_test(
                "c",
                TestResult { status: Some(Status::Passed), ..TestResult::EMPTY },
            ),
            ListenerEvent::finish_all_test(),
        ];

        let expected_events = vec![
            ListenerEvent::start_test("a"),
            ListenerEvent::finish_test(
                "a",
                TestResult { status: Some(Status::Passed), ..TestResult::EMPTY },
            ),
            ListenerEvent::start_test("b"),
            ListenerEvent::finish_test(
                "b",
                TestResult { status: Some(Status::Passed), ..TestResult::EMPTY },
            ),
            ListenerEvent::start_test("c"),
            ListenerEvent::finish_test(
                "c",
                TestResult { status: Some(Status::Passed), ..TestResult::EMPTY },
            ),
            ListenerEvent::finish_all_test(),
        ];
        events.sort();
        assert_eq!(events, expected_events);
    }
}
