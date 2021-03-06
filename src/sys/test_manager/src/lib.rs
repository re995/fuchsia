// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use {
    crate::{diagnostics::IsolatedLogsProvider, error::*},
    anyhow::{format_err, Context, Error},
    cm_rust,
    diagnostics_bridge::ArchiveReaderManager,
    fdiagnostics::ArchiveAccessorProxy,
    fidl::endpoints::ServerEnd,
    fidl_fuchsia_diagnostics as fdiagnostics, fidl_fuchsia_io2 as fio2, fidl_fuchsia_test as ftest,
    fidl_fuchsia_test_internal as ftest_internal, fidl_fuchsia_test_manager as ftest_manager,
    ftest::{Invocation, SuiteMarker},
    ftest_manager::{
        CaseStatus, LaunchError, RunControllerRequest, RunControllerRequestStream,
        SuiteControllerRequest, SuiteControllerRequestStream, SuiteEvent as FidlSuiteEvent,
        SuiteEventPayload as FidlSuiteEventPayload, SuiteStatus,
    },
    fuchsia_async as fasync,
    fuchsia_component::server::ServiceFs,
    fuchsia_component_test::{
        builder::{
            Capability, CapabilityRoute, ComponentSource, Event, RealmBuilder, RouteEndpoint,
        },
        error::Error as RealmBuilderError,
        mock::{Mock, MockHandles},
        Realm, RealmInstance,
    },
    fuchsia_zircon as zx,
    futures::{
        channel::{mpsc, oneshot},
        future::join_all,
        lock,
        prelude::*,
        StreamExt,
    },
    lazy_static::lazy_static,
    regex::Regex,
    std::collections::{HashMap, HashSet},
    std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex, Weak,
    },
    tracing::{debug, error, warn},
};

mod diagnostics;
mod error;

pub const TEST_ROOT_REALM_NAME: &'static str = "test_root";
pub const WRAPPER_ROOT_REALM_PATH: &'static str = "test_wrapper/test_root";
pub const ARCHIVIST_REALM_PATH: &'static str = "test_wrapper/archivist";

lazy_static! {
    static ref ARCHIVIST_FOR_EMBEDDING_URL: &'static str =
        "fuchsia-pkg://fuchsia.com/test_manager#meta/archivist-for-embedding.cm";
    static ref READ_RIGHTS: fio2::Operations = fio2::Operations::Connect
        | fio2::Operations::Enumerate
        | fio2::Operations::Traverse
        | fio2::Operations::ReadBytes
        | fio2::Operations::GetAttributes;
    static ref READ_WRITE_RIGHTS: fio2::Operations = fio2::Operations::Connect
        | fio2::Operations::Enumerate
        | fio2::Operations::Traverse
        | fio2::Operations::ReadBytes
        | fio2::Operations::WriteBytes
        | fio2::Operations::ModifyDirectory
        | fio2::Operations::GetAttributes
        | fio2::Operations::UpdateAttributes;
    static ref ADMIN_RIGHTS: fio2::Operations = fio2::Operations::Admin;
}

struct TestMapValue {
    test_url: String,
    can_be_deleted: bool,
    last_accessed: fasync::Time,
}

/// Cache mapping test realm name to test url.
/// This cache will run cleanup on constant intervals and delete entries which have been marked as
/// stale and not accessed for `cleanup_interval` duration.
/// We don't delete entries as soon as they are marked as stale because dependent
/// service might still be processing requests.
pub struct TestMap {
    /// Key: test realm name
    test_map: Mutex<HashMap<String, TestMapValue>>,

    /// Interval after which cleanup is fired.
    cleanup_interval: zx::Duration,
}

impl TestMap {
    /// Create new instance of this object, wrap it in Arc and return.
    /// 'cleanup_interval': Intervals after which cleanup should fire.
    pub fn new(cleanup_interval: zx::Duration) -> Arc<Self> {
        let s = Arc::new(Self { test_map: Mutex::new(HashMap::new()), cleanup_interval });
        let weak = Arc::downgrade(&s);
        let d = s.cleanup_interval.clone();
        fasync::Task::spawn(async move {
            let mut interval = fasync::Interval::new(d);
            while let Some(_) = interval.next().await {
                if let Some(s) = weak.upgrade() {
                    s.run_cleanup();
                } else {
                    break;
                }
            }
        })
        .detach();
        s
    }

    fn run_cleanup(&self) {
        let mut test_map = self.test_map.lock().unwrap();
        test_map.retain(|_, v| {
            !(v.can_be_deleted && (v.last_accessed < fasync::Time::now() - self.cleanup_interval))
        });
    }

    /// Insert into the cache. If key was already present will return old value.
    pub fn insert(&self, test_name: String, test_url: String) -> Option<String> {
        let mut test_map = self.test_map.lock().unwrap();
        test_map
            .insert(
                test_name,
                TestMapValue {
                    test_url,
                    can_be_deleted: false,
                    last_accessed: fasync::Time::now(),
                },
            )
            .map(|v| v.test_url)
    }

    /// Get `test_url` if present in the map.
    pub fn get(&self, k: &str) -> Option<String> {
        let mut test_map = self.test_map.lock().unwrap();
        match test_map.get_mut(k) {
            Some(v) => {
                v.last_accessed = fasync::Time::now();
                return Some(v.test_url.clone());
            }
            None => {
                return None;
            }
        }
    }

    /// Delete cache entry without marking it as stale and waiting for cleanup.
    pub fn delete(&self, k: &str) {
        let mut test_map = self.test_map.lock().unwrap();
        test_map.remove(k);
    }

    /// Mark cache entry as stale which would be deleted in future if not accessed.
    pub fn mark_as_stale(&self, k: &str) {
        let mut test_map = self.test_map.lock().unwrap();
        if let Some(v) = test_map.get_mut(k) {
            v.can_be_deleted = true;
        }
    }
}

struct Suite {
    test_url: String,
    options: ftest_manager::RunOptions,
    controller: SuiteControllerRequestStream,
}
struct TestSuiteBuilder {
    suites: Vec<Suite>,
}

impl TestSuiteBuilder {
    async fn run_controller(
        mut controller: RunControllerRequestStream,
        run_task: fasync::Task<()>,
        stop_sender: oneshot::Sender<()>,
    ) {
        let mut task = Some(run_task);
        let mut stop_sender = Some(stop_sender);
        // no need to check controller error.
        while let Ok(Some(request)) = controller.try_next().await {
            match request {
                RunControllerRequest::Stop { .. } => {
                    if let Some(stop_sender) = stop_sender.take() {
                        // no need to check error.
                        let _ = stop_sender.send(());
                        // after this all `senders` go away and subsequent GetEvent call will
                        // return rest of events and eventually a empty array and will close the
                        // connection after that.
                    }
                }
                RunControllerRequest::Kill { .. } => {
                    if let Some(task) = task.take() {
                        task.cancel().await;
                    }
                    // after this all `senders` go away and subsequent GetEvent call will
                    // return rest of events and eventually a empty array and will close the
                    // connection after that.
                }
                RunControllerRequest::GetEvents { responder } => {
                    task = fasync::Task::spawn(async move {
                        if let Some(t) = task.take() {
                            t.await;
                        }
                        let events: Vec<ftest_manager::RunEvent> = vec![];
                        // maybe client disconnected, no need to check error.
                        let _ = responder.send(&mut events.into_iter());
                    })
                    .into();
                }
            }
        }

        if let Some(task) = task.take() {
            task.cancel().await;
        }
    }
    async fn run(self, controller: RunControllerRequestStream, test_map: Arc<TestMap>) {
        let (stop_sender, mut stop_recv) = oneshot::channel::<()>();
        let task = fuchsia_async::Task::spawn(async move {
            // run test suites serially for now
            for suite in self.suites {
                // only check before running the test. We should complete the test run for
                // running tests, if stop is called.
                if let Ok(Some(())) = stop_recv.try_recv() {
                    break;
                }
                suite.run(test_map.clone()).await;
            }
        });
        Self::run_controller(controller, task, stop_sender).await;
    }
}

fn suite_error(err: fidl::Error) -> anyhow::Error {
    match err {
        fidl::Error::ClientChannelClosed { .. } => anyhow::anyhow!(
            "The test protocol was closed. This may mean `fuchsia.test.Suite` was not \
            configured correctly. Refer to: \
            https://fuchsia.dev/fuchsia-src/development/components/v2/troubleshooting#troubleshoot-test"
        ),
        err => err.into(),
    }
}

/// Enumerates test and return invocations.
async fn enumerate_tests(
    suite: &ftest::SuiteProxy,
    patterns: Option<Vec<glob::Pattern>>,
) -> Result<Vec<Invocation>, anyhow::Error> {
    debug!("enumerating tests");
    let (case_iterator, server_end) =
        fidl::endpoints::create_proxy().expect("cannot create case iterator");
    suite.get_tests(server_end).map_err(suite_error)?;
    let mut invocations = vec![];

    loop {
        let cases = case_iterator.get_next().await.map_err(suite_error)?;
        if cases.is_empty() {
            break;
        }
        for case in cases {
            let case_name = case.name.ok_or(format_err!("invocation should contain a name."))?;
            if patterns.as_ref().map_or(true, |p| p.iter().any(|p| p.matches(&case_name))) {
                invocations.push(Invocation {
                    name: Some(case_name),
                    tag: None,
                    ..Invocation::EMPTY
                });
            }
        }
    }

    debug!("invocations: {:#?}", invocations);

    Ok(invocations)
}

fn get_glob_patterns(
    filters: &Option<Vec<String>>,
) -> Result<Option<Vec<glob::Pattern>>, anyhow::Error> {
    filters
        .as_ref()
        .map(|filters| {
            let mut v = vec![];
            filters.iter().try_for_each(|filter| {
                let p = glob::Pattern::new(&filter)
                    .map_err(|e| format_err!("Bad test filter pattern: {}", e))?;
                v.push(p);
                Ok::<(), anyhow::Error>(())
            })?;
            Ok(v)
        })
        .transpose()
}

fn get_invocation_options(options: ftest_manager::RunOptions) -> ftest::RunOptions {
    ftest::RunOptions {
        include_disabled_tests: options.run_disabled_tests,
        parallel: options.parallel,
        arguments: options.arguments,
        ..ftest::RunOptions::EMPTY
    }
}

async fn run_suite(
    test_url: String,
    suite: ftest::SuiteProxy,
    options: ftest_manager::RunOptions,
    mut sender: mpsc::Sender<Result<FidlSuiteEvent, LaunchError>>,
    mut stop_recv: oneshot::Receiver<()>,
) {
    debug!("running test suite {}", test_url);

    let fut = async {
        let patterns = match get_glob_patterns(&options.case_filters_to_run) {
            Ok(p) => p,
            Err(e) => {
                sender.send(Err(LaunchError::InvalidArgs)).await.unwrap();
                return Err(e);
            }
        };

        let invocations = match enumerate_tests(&suite, patterns).await {
            Ok(i) => i,
            Err(e) => {
                sender.send(Err(LaunchError::CaseEnumeration)).await.unwrap();
                return Err(e);
            }
        };
        if let Ok(Some(_)) = stop_recv.try_recv() {
            sender
                .send(Ok(SuiteEvents::suite_finished(SuiteStatus::Stopped).into()))
                .await
                .unwrap();
            return Ok(());
        }

        let mut suite_status = SuiteStatus::Passed;
        let mut invocations_iter = invocations.into_iter();
        let counter = AtomicU32::new(0);
        let timeout_time = match options.timeout {
            Some(t) => zx::Time::after(zx::Duration::from_nanos(t)),
            None => zx::Time::INFINITE,
        };
        let timeout_fut = fasync::Timer::new(timeout_time).shared();

        let run_options = get_invocation_options(options);

        loop {
            const INVOCATIONS_CHUNK: usize = 50;
            let chunk = invocations_iter.by_ref().take(INVOCATIONS_CHUNK).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
            let res = match run_invocations(
                &suite,
                chunk,
                run_options.clone(),
                &counter,
                &mut sender,
                timeout_fut.clone(),
            )
            .await
            .context("Error running test cases")
            {
                Ok(success) => success,
                Err(e) => {
                    return Err(e);
                }
            };
            if res == SuiteStatus::TimedOut {
                sender
                    .send(Ok(SuiteEvents::suite_finished(SuiteStatus::TimedOut).into()))
                    .await
                    .unwrap();
                return Ok(());
            }
            suite_status = concat_suite_status(suite_status, res);
            if let Ok(Some(_)) = stop_recv.try_recv() {
                sender
                    .send(Ok(SuiteEvents::suite_finished(SuiteStatus::Stopped).into()))
                    .await
                    .unwrap();
                return Ok(());
            }
        }
        sender.send(Ok(SuiteEvents::suite_finished(suite_status).into())).await.unwrap();
        Ok(())
    };
    if let Err(e) = fut.await {
        warn!("Error running test {}: {:?}", test_url, e);
    }
}

fn concat_suite_status(initial: SuiteStatus, new: SuiteStatus) -> SuiteStatus {
    if initial.into_primitive() > new.into_primitive() {
        return initial;
    }
    return new;
}

enum SuiteEventPayload {
    CaseFound(String, u32),
    CaseStarted(u32),
    CaseStopped(u32, CaseStatus),
    CaseFinished(u32),
    CaseStdout(u32, zx::Socket),
    SuiteSyslog(ftest_manager::Syslog),
    SuiteFinished(SuiteStatus),
}
struct SuiteEvents {
    timestamp: i64,
    payload: SuiteEventPayload,
}

impl Into<FidlSuiteEvent> for SuiteEvents {
    fn into(self) -> FidlSuiteEvent {
        match self.payload {
            SuiteEventPayload::CaseFound(name, identifier) => FidlSuiteEvent {
                timestamp: Some(self.timestamp),
                payload: Some(FidlSuiteEventPayload::CaseFound(ftest_manager::CaseFound {
                    test_case_name: name,
                    identifier,
                })),
                ..FidlSuiteEvent::EMPTY
            },
            SuiteEventPayload::CaseStarted(identifier) => FidlSuiteEvent {
                timestamp: Some(self.timestamp),
                payload: Some(FidlSuiteEventPayload::CaseStarted(ftest_manager::CaseStarted {
                    identifier,
                })),
                ..FidlSuiteEvent::EMPTY
            },
            SuiteEventPayload::CaseStopped(identifier, status) => FidlSuiteEvent {
                timestamp: Some(self.timestamp),
                payload: Some(FidlSuiteEventPayload::CaseStopped(ftest_manager::CaseStopped {
                    identifier,
                    status,
                })),
                ..FidlSuiteEvent::EMPTY
            },
            SuiteEventPayload::CaseFinished(identifier) => FidlSuiteEvent {
                timestamp: Some(self.timestamp),
                payload: Some(FidlSuiteEventPayload::CaseFinished(ftest_manager::CaseFinished {
                    identifier,
                })),
                ..FidlSuiteEvent::EMPTY
            },
            SuiteEventPayload::CaseStdout(identifier, socket) => FidlSuiteEvent {
                timestamp: Some(self.timestamp),
                payload: Some(FidlSuiteEventPayload::CaseArtifact(ftest_manager::CaseArtifact {
                    identifier,
                    artifact: ftest_manager::Artifact::Stdout(socket),
                })),
                ..FidlSuiteEvent::EMPTY
            },
            SuiteEventPayload::SuiteSyslog(syslog) => FidlSuiteEvent {
                timestamp: Some(self.timestamp),
                payload: Some(FidlSuiteEventPayload::SuiteArtifact(ftest_manager::SuiteArtifact {
                    artifact: ftest_manager::Artifact::Log(syslog),
                })),
                ..FidlSuiteEvent::EMPTY
            },
            SuiteEventPayload::SuiteFinished(status) => FidlSuiteEvent {
                timestamp: Some(self.timestamp),
                payload: Some(FidlSuiteEventPayload::SuiteFinished(ftest_manager::SuiteFinished {
                    status,
                })),
                ..FidlSuiteEvent::EMPTY
            },
        }
    }
}

impl SuiteEvents {
    fn case_found(identifier: u32, name: String) -> Self {
        Self {
            timestamp: zx::Time::get_monotonic().into_nanos(),
            payload: SuiteEventPayload::CaseFound(name, identifier),
        }
    }

    fn case_started(identifier: u32) -> Self {
        Self {
            timestamp: zx::Time::get_monotonic().into_nanos(),
            payload: SuiteEventPayload::CaseStarted(identifier),
        }
    }

    fn case_stopped(identifier: u32, status: CaseStatus) -> Self {
        Self {
            timestamp: zx::Time::get_monotonic().into_nanos(),
            payload: SuiteEventPayload::CaseStopped(identifier, status),
        }
    }

    fn case_finished(identifier: u32) -> Self {
        Self {
            timestamp: zx::Time::get_monotonic().into_nanos(),
            payload: SuiteEventPayload::CaseFinished(identifier),
        }
    }

    fn case_stdout(identifier: u32, socket: zx::Socket) -> Self {
        Self {
            timestamp: zx::Time::get_monotonic().into_nanos(),
            payload: SuiteEventPayload::CaseStdout(identifier, socket),
        }
    }

    fn suite_syslog(syslog: ftest_manager::Syslog) -> Self {
        Self {
            timestamp: zx::Time::get_monotonic().into_nanos(),
            payload: SuiteEventPayload::SuiteSyslog(syslog),
        }
    }

    fn suite_finished(status: SuiteStatus) -> Self {
        Self {
            timestamp: zx::Time::get_monotonic().into_nanos(),
            payload: SuiteEventPayload::SuiteFinished(status),
        }
    }

    #[cfg(test)]
    fn into_suite_run_event(self) -> FidlSuiteEvent {
        self.into()
    }
}

/// Runs the test component using `suite` and collects stdout logs and results.
async fn run_invocations(
    suite: &ftest::SuiteProxy,
    invocations: Vec<Invocation>,
    run_options: fidl_fuchsia_test::RunOptions,
    counter: &AtomicU32,
    sender: &mut mpsc::Sender<Result<FidlSuiteEvent, LaunchError>>,
    timeout_fut: futures::future::Shared<fasync::Timer>,
) -> Result<SuiteStatus, anyhow::Error> {
    let (run_listener_client, mut run_listener) =
        fidl::endpoints::create_request_stream().expect("cannot create request stream");
    suite.run(&mut invocations.into_iter().map(|i| i.into()), run_options, run_listener_client)?;

    let tasks = Arc::new(lock::Mutex::new(vec![]));
    let running_test_cases = Arc::new(lock::Mutex::new(HashSet::new()));
    let tasks_clone = tasks.clone();
    let initial_suite_status: SuiteStatus;
    let mut sender_clone = sender.clone();
    let test_fut = async {
        let mut initial_suite_status = SuiteStatus::DidNotFinish;
        while let Some(result_event) =
            run_listener.try_next().await.context("error waiting for listener")?
        {
            match result_event {
                ftest::RunListenerRequest::OnTestCaseStarted {
                    invocation,
                    primary_log,
                    listener,
                    control_handle: _,
                } => {
                    let name =
                        invocation.name.ok_or(format_err!("cannot find name in invocation"))?;
                    let identifier = counter.fetch_add(1, Ordering::Relaxed);
                    let events = vec![
                        Ok(SuiteEvents::case_found(identifier, name).into()),
                        Ok(SuiteEvents::case_started(identifier).into()),
                        Ok(SuiteEvents::case_stdout(identifier, primary_log).into()),
                    ];
                    for event in events {
                        sender_clone.send(event).await.unwrap();
                    }
                    let listener =
                        listener.into_stream().context("Cannot convert listener to stream")?;
                    running_test_cases.lock().await.insert(identifier);
                    let running_test_cases = running_test_cases.clone();
                    let sender = sender_clone.clone();
                    let task = fasync::Task::spawn(async move {
                        let status = listen_for_completion(listener, identifier, sender).await;
                        running_test_cases.lock().await.remove(&identifier);
                        status
                    });
                    tasks_clone.lock().await.push(task);
                }
                ftest::RunListenerRequest::OnFinished { .. } => {
                    initial_suite_status = SuiteStatus::Passed;
                    break;
                }
            }
        }
        Ok(initial_suite_status)
    }
    .fuse();

    futures::pin_mut!(test_fut);
    let timeout_fut = timeout_fut.fuse();
    futures::pin_mut!(timeout_fut);

    futures::select! {
        () = timeout_fut => {
                let mut all_tasks = vec![];
                let mut tasks = tasks.lock().await;
                all_tasks.append(&mut tasks);
                drop(tasks);
                for t in all_tasks {
                    t.cancel().await;
                }
                let running_test_cases = running_test_cases.lock().await;
                for i in &*running_test_cases {
                    sender
                        .send(Ok(SuiteEvents::case_stopped(*i, CaseStatus::TimedOut).into()))
                        .await
                        .unwrap();
                    sender
                        .send(Ok(SuiteEvents::case_finished(*i).into()))
                        .await
                        .unwrap();
                }
                return Ok(SuiteStatus::TimedOut);
            }
        r = test_fut => {
            initial_suite_status = match r {
                Err(e) => {
                    return Err(e);
                }
                Ok(s) => s,
            };
        }
    }

    let mut tasks = tasks.lock().await;
    let mut all_tasks = vec![];
    all_tasks.append(&mut tasks);
    // await for all invocations to complete for which test case never completed.
    let suite_status = join_all(all_tasks)
        .await
        .into_iter()
        .fold(initial_suite_status, get_suite_status_from_case_status);
    Ok(suite_status)
}

fn get_suite_status_from_case_status(
    initial_suite_status: SuiteStatus,
    case_status: CaseStatus,
) -> SuiteStatus {
    let status = match case_status {
        CaseStatus::Passed => SuiteStatus::Passed,
        CaseStatus::Failed => SuiteStatus::Failed,
        CaseStatus::TimedOut => SuiteStatus::TimedOut,
        CaseStatus::Skipped => SuiteStatus::Passed,
        CaseStatus::Error => SuiteStatus::Failed,
        _ => {
            panic!("this should not happen");
        }
    };
    concat_suite_status(initial_suite_status, status)
}

/// Listen for test completion on the given |listener|. Returns None if the channel is closed
/// before a test completion event.
async fn listen_for_completion(
    mut listener: ftest::CaseListenerRequestStream,
    case_identifier: u32,
    mut sender: mpsc::Sender<Result<FidlSuiteEvent, LaunchError>>,
) -> CaseStatus {
    let status = match listener.try_next().await.context("waiting for listener") {
        Ok(Some(request)) => {
            let ftest::CaseListenerRequest::Finished { result, control_handle: _ } = request;
            let result = match result.status {
                Some(status) => match status {
                    fidl_fuchsia_test::Status::Passed => CaseStatus::Passed,
                    fidl_fuchsia_test::Status::Failed => CaseStatus::Failed,
                    fidl_fuchsia_test::Status::Skipped => CaseStatus::Skipped,
                },
                // This will happen when test protocol is not properly implemented
                // by the test and it forgets to set the result.
                None => CaseStatus::Error,
            };
            result
        }
        Err(e) => {
            warn!("listener failed: {:?}", e);
            CaseStatus::Error
        }
        Ok(None) => CaseStatus::Error,
    };

    sender
        .send(Ok(SuiteEvents::case_stopped(case_identifier, status.clone()).into()))
        .await
        .unwrap();
    sender.send(Ok(SuiteEvents::case_finished(case_identifier).into())).await.unwrap();
    status
}

// max events to send so that we don't cross fidl limits.
// TODO(anmittal): Use tape measure to calculate limit.
const EVENTS_THRESHOLD: usize = 50;

impl Suite {
    async fn run_controller(
        mut controller: SuiteControllerRequestStream,
        stop_sender: oneshot::Sender<()>,
        event_recv: mpsc::Receiver<Result<FidlSuiteEvent, LaunchError>>,
        mut test_run_task: Option<fasync::Task<()>>,
        mut running_test: Option<RunningTest>,
    ) -> Result<(), Error> {
        let mut event_recv = event_recv.into_stream().fuse();
        let mut stop_sender = Some(stop_sender);
        'controller_loop: while let Some(event) =
            controller.try_next().await.context("error running controller")?
        {
            match event {
                SuiteControllerRequest::Stop { .. } => {
                    // no need to handle error as task might already have finished.
                    if let Some(stop) = stop_sender.take() {
                        let _ = stop.send(());
                        // after this all `senders` go away and subsequent GetEvent call will
                        // return rest of event. Eventually an empty array and will close the
                        // connection after that.
                    }
                }
                SuiteControllerRequest::GetEvents { responder } => {
                    let mut events = vec![];

                    // wait for first event
                    let mut e = event_recv.next().await;

                    while let Some(event) = e {
                        match event {
                            Ok(event) => {
                                events.push(event);
                            }
                            Err(err) => {
                                responder
                                    .send(&mut Err(err))
                                    .map_err(TestManagerError::Response)?;
                                break 'controller_loop;
                            }
                        }
                        if events.len() >= EVENTS_THRESHOLD {
                            responder.send(&mut Ok(events)).map_err(TestManagerError::Response)?;
                            continue 'controller_loop;
                        }
                        e = match event_recv.next().now_or_never() {
                            Some(e) => e,
                            None => break,
                        }
                    }

                    let len = events.len();
                    responder.send(&mut Ok(events)).map_err(TestManagerError::Response)?;
                    if len == 0 {
                        break;
                    }
                }
                SuiteControllerRequest::Kill { .. } => {
                    if let Some(t) = test_run_task.take() {
                        t.cancel().await;
                    }
                    if let Some(test) = running_test.take() {
                        test.destroy().await;
                    }
                    // after this all `senders` go away and subsequent GetEvent call will
                    // return rest of event. Eventually an empty array and will close the
                    // connection after that.
                }
            }
        }

        // cancel running test (if it is still running)
        if let Some(t) = test_run_task.take() {
            t.cancel().await;
        }
        if let Some(test) = running_test.take() {
            test.destroy().await;
        }
        Ok(())
    }
    async fn run(self, test_map: Arc<TestMap>) {
        let (sender, recv) = mpsc::channel(1024);
        let (stop_sender, stop_recv) = oneshot::channel::<()>();
        let (suite, suite_request) = fidl::endpoints::create_proxy().expect("cannot create suite");
        let test = self.launch_test(sender.clone(), suite_request, test_map.clone()).await;
        let test_name = match &test {
            Some(t) => Some(t.instance.root.child_name()),
            None => None,
        };

        let task = match &test {
            Some(_) => Some(fasync::Task::spawn(run_suite(
                self.test_url.clone(),
                suite,
                self.options,
                sender,
                stop_recv,
            ))),
            None => None,
        };
        let controller_ret =
            Self::run_controller(self.controller, stop_sender, recv, task, test).await;

        if let Some(test_name) = test_name {
            test_map.mark_as_stale(&test_name);
        }
        if let Err(e) = controller_ret {
            warn!("Ended test {}: {:?}", self.test_url, e);
        }
    }

    async fn launch_test(
        &self,
        mut sender: mpsc::Sender<Result<FidlSuiteEvent, LaunchError>>,
        suite_request: ServerEnd<SuiteMarker>,
        test_map: Arc<TestMap>,
    ) -> Option<RunningTest> {
        let (log_iterator, syslog) = match self.options.log_iterator {
            Some(ftest_manager::LogsIteratorOption::ArchiveIterator) => {
                let (proxy, request) =
                    fidl::endpoints::create_endpoints().expect("cannot create suite");
                (
                    ftest_manager::LogsIterator::Archive(request),
                    ftest_manager::Syslog::Archive(proxy),
                )
            }
            _ => {
                let (proxy, request) =
                    fidl::endpoints::create_endpoints().expect("cannot create suite");
                (ftest_manager::LogsIterator::Batch(request), ftest_manager::Syslog::Batch(proxy))
            }
        };

        sender.send(Ok(SuiteEvents::suite_syslog(syslog).into())).await.unwrap();
        match launch_test(&self.test_url, suite_request, test_map, Some(log_iterator)).await {
            Ok(test) => Some(test),
            Err(err) => {
                warn!(?err, "Failed to launch test");
                sender.send(Err(err.into())).await.expect("Receiver cannot be dead");
                sender.close_channel();
                None
            }
        }
    }
}

/// Start test manager and serve it over `stream`.
pub async fn run_test_manager(
    mut stream: ftest_manager::RunBuilderRequestStream,
    test_map: Arc<TestMap>,
) -> Result<(), TestManagerError> {
    let mut builder = TestSuiteBuilder { suites: vec![] };
    while let Some(event) = stream.try_next().await.map_err(TestManagerError::Stream)? {
        match event {
            ftest_manager::RunBuilderRequest::AddSuite {
                test_url,
                options,
                controller,
                control_handle,
            } => {
                let controller = match controller.into_stream() {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(
                            "Cannot add suite {}, invalid controller. Closing connection. error: {}",
                            test_url,e
                        );
                        control_handle.shutdown_with_epitaph(zx::Status::BAD_HANDLE);
                        break;
                    }
                };
                builder.suites.push(Suite { test_url, options, controller });
            }
            ftest_manager::RunBuilderRequest::Build { controller, control_handle } => {
                let controller = match controller.into_stream() {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Invalid builder controller. Closing connection. error: {}", e);
                        control_handle.shutdown_with_epitaph(zx::Status::BAD_HANDLE);
                        break;
                    }
                };
                builder.run(controller, test_map).await;
                // clients needs to reconnect to run new tests.
                break;
            }
        }
    }
    Ok(())
}

/// Start test manager and serve it over `stream`.
pub async fn run_test_manager_query_server(
    mut stream: ftest_manager::QueryRequestStream,
    test_map: Arc<TestMap>,
) -> Result<(), TestManagerError> {
    while let Some(event) = stream.try_next().await.map_err(TestManagerError::Stream)? {
        match event {
            ftest_manager::QueryRequest::Enumerate { test_url, iterator, responder } => {
                let mut iterator = match iterator.into_stream() {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Cannot query test, invalid iterator {}: {}", test_url, e);
                        let _ = responder.send(&mut Err(LaunchError::InvalidArgs));
                        break;
                    }
                };
                let (suite, suite_request) =
                    fidl::endpoints::create_proxy().expect("cannot create suite proxy");
                match launch_test(&test_url, suite_request, test_map.clone(), None).await {
                    Ok(test) => {
                        let enumeration_result = enumerate_tests(&suite, None).await;
                        let test_name = test.instance.root.child_name();
                        let t = fasync::Task::spawn(test.destroy());
                        match enumeration_result {
                            Ok(invocations) => {
                                const NAMES_CHUNK: usize = 50;
                                let mut names = Vec::with_capacity(invocations.len());
                                if let Ok(_) =
                                    invocations.into_iter().try_for_each(|i| match i.name {
                                        Some(name) => {
                                            names.push(name);
                                            Ok(())
                                        }
                                        None => {
                                            warn!("no name for a invocation in {}", test_url);
                                            Err(())
                                        }
                                    })
                                {
                                    let _ = responder.send(&mut Ok(()));
                                    let mut names = names.chunks(NAMES_CHUNK);
                                    while let Ok(Some(request)) = iterator.try_next().await {
                                        match request {
                                            ftest_manager::CaseIteratorRequest::GetNext {
                                                responder,
                                            } => match names.next() {
                                                Some(names) => {
                                                    let _ =
                                                        responder.send(&mut names.into_iter().map(
                                                            |s| ftest_manager::Case {
                                                                name: Some(s.into()),
                                                                ..ftest_manager::Case::EMPTY
                                                            },
                                                        ));
                                                }
                                                None => {
                                                    let _ = responder.send(&mut vec![].into_iter());
                                                }
                                            },
                                        }
                                    }
                                } else {
                                    let _ = responder.send(&mut Err(LaunchError::CaseEnumeration));
                                }
                            }
                            Err(e) => {
                                warn!("cannot enumerate tests for {}: {:?}", test_url, e);
                                let _ = responder.send(&mut Err(LaunchError::CaseEnumeration));
                            }
                        }
                        t.await;
                        test_map.mark_as_stale(&test_name);
                    }
                    Err(e) => {
                        let _ = responder.send(&mut Err(e.into()));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Start test manager info server and serve it over `stream`.
pub async fn run_test_manager_info_server(
    mut stream: ftest_internal::InfoRequestStream,
    test_map: Arc<TestMap>,
) -> Result<(), TestManagerError> {
    // This ensures all monikers are relative to test_manager and supports capturing the top-level
    // name of the test realm.
    let re = Regex::new(r"^\./tests:(.*?):.*$").unwrap();
    while let Some(event) = stream.try_next().await.map_err(TestManagerError::Stream)? {
        match event {
            ftest_internal::InfoRequest::GetTestUrl { moniker, responder } => {
                if !re.is_match(&moniker) {
                    responder
                        .send(&mut Err(zx::sys::ZX_ERR_NOT_SUPPORTED))
                        .map_err(TestManagerError::Response)?;
                    continue;
                }

                let cap = re.captures(&moniker).unwrap();
                if let Some(s) = test_map.get(&cap[1]) {
                    responder.send(&mut Ok(s)).map_err(TestManagerError::Response)?;
                } else {
                    responder
                        .send(&mut Err(zx::sys::ZX_ERR_NOT_FOUND))
                        .map_err(TestManagerError::Response)?;
                }
            }
        }
    }
    Ok(())
}

struct RunningTest {
    instance: RealmInstance,
    logs_iterator_task: Option<fasync::Task<Result<(), anyhow::Error>>>,

    // safe keep archive accessor which tests might use.
    archive_accessor: Arc<ArchiveAccessorProxy>,
}

impl RunningTest {
    async fn destroy(mut self) {
        let destroy_waiter = self.instance.root.take_destroy_waiter();
        drop(self.instance);
        // When serving logs over ArchiveIterator in the host, we should also wait for all logs to
        // be drained.
        drop(self.archive_accessor);
        if let Some(task) = self.logs_iterator_task {
            task.await.unwrap_or_else(|err| {
                error!(?err, "Failed to await for logs streaming task");
            });
        }

        destroy_waiter.await.unwrap_or_else(|err| {
            error!(?err, "Failed to destroy instance");
        });
    }
}

/// Launch test and return the name of test used to launch it in collection.
async fn launch_test(
    test_url: &str,
    suite_request: ServerEnd<SuiteMarker>,
    test_map: Arc<TestMap>,
    log_iterator: Option<ftest_manager::LogsIterator>,
) -> Result<RunningTest, LaunchTestError> {
    // This archive accessor will be served by the embedded archivist.
    let (archive_accessor, archive_accessor_server_end) =
        fidl::endpoints::create_proxy::<fdiagnostics::ArchiveAccessorMarker>()
            .map_err(LaunchTestError::CreateProxyForArchiveAccessor)?;

    let archive_accessor_arc = Arc::new(archive_accessor);
    let mut realm = get_realm(Arc::downgrade(&archive_accessor_arc), test_url)
        .await
        .map_err(LaunchTestError::InitializeTestRealm)?;
    realm.set_collection_name("tests");
    let instance = realm.create().await.map_err(LaunchTestError::CreateTestRealm)?;
    let test_name = instance.root.child_name();
    test_map.insert(test_name.clone(), test_url.to_string());
    let archive_accessor_arc_clone = archive_accessor_arc.clone();
    let connect_to_instance_services = async move {
        instance
            .root
            .connect_request_to_protocol_at_exposed_dir::<fdiagnostics::ArchiveAccessorMarker>(
                archive_accessor_server_end,
            )
            .map_err(LaunchTestError::ConnectToArchiveAccessor)?;

        let mut isolated_logs_provider = IsolatedLogsProvider::new(archive_accessor_arc_clone);
        let logs_iterator_task = match log_iterator {
            None => None,
            Some(ftest_manager::LogsIterator::Archive(iterator)) => {
                let task = isolated_logs_provider
                    .spawn_iterator_server(iterator)
                    .map_err(LaunchTestError::StreamIsolatedLogs)?;
                Some(task)
            }
            Some(ftest_manager::LogsIterator::Batch(iterator)) => {
                isolated_logs_provider
                    .start_streaming_logs(iterator)
                    .map_err(LaunchTestError::StreamIsolatedLogs)?;
                None
            }
            Some(_) => None,
        };

        instance
            .root
            .connect_request_to_protocol_at_exposed_dir(suite_request)
            .map_err(LaunchTestError::ConnectToTestSuite)?;
        Ok(RunningTest { instance, logs_iterator_task, archive_accessor: archive_accessor_arc })
    };

    let running_test_result = connect_to_instance_services.await;
    if running_test_result.is_err() {
        test_map.delete(&test_name);
    }
    running_test_result
}

async fn get_realm(
    archive_accessor: Weak<fdiagnostics::ArchiveAccessorProxy>,
    test_url: &str,
) -> Result<Realm, RealmBuilderError> {
    let mut builder = RealmBuilder::new().await?;
    builder
        .add_eager_component(WRAPPER_ROOT_REALM_PATH, ComponentSource::url(test_url))
        .await?
        .add_component(
            "mocks-server",
            ComponentSource::Mock(Mock::new(move |mock_handles| {
                Box::pin(serve_mocks(archive_accessor.clone(), mock_handles))
            })),
        )
        .await?
        .add_eager_component(
            ARCHIVIST_REALM_PATH,
            ComponentSource::url(*ARCHIVIST_FOR_EMBEDDING_URL),
        )
        .await?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.process.Launcher"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.boot.WriteOnlyLog"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.sys2.EventSource"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![
                RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH),
                RouteEndpoint::component(ARCHIVIST_REALM_PATH),
            ],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::storage("temp", "/tmp"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::storage("data", "/data"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::storage("cache", "/cache"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.logger.LogSink"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(ARCHIVIST_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.logger.LogSink"),
            source: RouteEndpoint::component(ARCHIVIST_REALM_PATH),
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.logger.Log"),
            source: RouteEndpoint::component(ARCHIVIST_REALM_PATH),
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.diagnostics.ArchiveAccessor"),
            source: RouteEndpoint::component("mocks-server"),
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.diagnostics.ArchiveAccessor"),
            source: RouteEndpoint::component(ARCHIVIST_REALM_PATH),
            targets: vec![RouteEndpoint::AboveRoot],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::Event(Event::Started, cm_rust::EventMode::Async),
            source: RouteEndpoint::component("test_wrapper"),
            targets: vec![RouteEndpoint::component(ARCHIVIST_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::Event(Event::Stopped, cm_rust::EventMode::Async),
            source: RouteEndpoint::component("test_wrapper"),
            targets: vec![RouteEndpoint::component(ARCHIVIST_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::Event(Event::Running, cm_rust::EventMode::Async),
            source: RouteEndpoint::component("test_wrapper"),
            targets: vec![RouteEndpoint::component(ARCHIVIST_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::Event(
                Event::directory_ready("diagnostics"),
                cm_rust::EventMode::Async,
            ),
            source: RouteEndpoint::component("test_wrapper"),
            targets: vec![RouteEndpoint::component(ARCHIVIST_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::Event(
                Event::capability_requested("fuchsia.logger.LogSink"),
                cm_rust::EventMode::Async,
            ),
            source: RouteEndpoint::component("test_wrapper"),
            targets: vec![RouteEndpoint::component(ARCHIVIST_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.test.Suite"),
            source: RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH),
            targets: vec![RouteEndpoint::AboveRoot],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.hardware.display.Provider"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.scheduler.ProfileProvider"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.sysmem.Allocator"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.tracing.provider.Registry"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("root-ssl-certificates", "", *READ_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("config-data", "", *READ_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory(
                "deprecated-tmp",
                "",
                *ADMIN_RIGHTS | *READ_WRITE_RIGHTS,
            ),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("dev-input-report", "", *READ_WRITE_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("dev-display-controller", "", *READ_WRITE_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("dev-goldfish-address-space", "", *READ_WRITE_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("dev-goldfish-control", "", *READ_WRITE_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("dev-goldfish-pipe", "", *READ_WRITE_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("dev-goldfish-sync", "", *READ_WRITE_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::directory("dev-gpu", "", *READ_WRITE_RIGHTS),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.vulkan.loader.Loader"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.kernel.VmexResource"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        // The two following are for v1 components being run by the nested realm builder
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.sys.Loader"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?
        .add_route(CapabilityRoute {
            capability: Capability::protocol("fuchsia.sys.Environment"),
            source: RouteEndpoint::AboveRoot,
            targets: vec![RouteEndpoint::component(WRAPPER_ROOT_REALM_PATH)],
        })?;

    Ok(builder.build())
}

async fn serve_mocks(
    archive_accessor: Weak<fdiagnostics::ArchiveAccessorProxy>,
    mock_handles: MockHandles,
) -> Result<(), Error> {
    let mut fs = ServiceFs::new();
    fs.dir("svc").add_fidl_service(move |stream| {
        let archive_accessor_clone = archive_accessor.clone();
        fasync::Task::spawn(async move {
            diagnostics::run_intermediary_archive_accessor(archive_accessor_clone, stream)
                .await
                .unwrap_or_else(|e| {
                    warn!("Couldn't run proxied ArchiveAccessor: {:?}", e);
                })
        })
        .detach()
    });
    fs.serve_connection(mock_handles.outgoing_dir.into_channel())?;
    fs.collect::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use {
        super::*, fasync::pin_mut, fidl::endpoints::create_proxy_and_stream,
        ftest_internal::InfoMarker, matches::assert_matches, std::ops::Add, zx::DurationNum,
    };

    #[fasync::run_singlethreaded(test)]
    async fn test_info_server() {
        let (proxy, stream) = create_proxy_and_stream::<InfoMarker>().unwrap();
        let test_map = TestMap::new(10.seconds());
        let test_map_clone = test_map.clone();
        fasync::Task::local(async move {
            run_test_manager_info_server(stream, test_map_clone).await.unwrap()
        })
        .detach();
        test_map.insert("my_test".into(), "my_test_url".into());
        assert_eq!(
            proxy.get_test_url("./tests:not_available_realm:0/test_wrapper").await.unwrap(),
            Err(zx::sys::ZX_ERR_NOT_FOUND)
        );
        assert_eq!(
            proxy.get_test_url("./tests:my_test:0/test_wrapper").await.unwrap(),
            Ok("my_test_url".into())
        );
        assert_eq!(
            proxy.get_test_url("./tests:my_test:0/test_wrapper:0/my_component:0").await.unwrap(),
            Ok("my_test_url".into())
        );
        assert_eq!(
            proxy.get_test_url("./tests/my_test:0/test_wrapper:0/my_component:0").await.unwrap(),
            Err(zx::sys::ZX_ERR_NOT_SUPPORTED)
        );
        assert_eq!(
            proxy.get_test_url("/tests:my_test:0/test_wrapper:0/my_component:0").await.unwrap(),
            Err(zx::sys::ZX_ERR_NOT_SUPPORTED)
        );
    }

    async fn dummy_fn() {}

    #[test]
    fn test_map_works() {
        let mut executor = fasync::TestExecutor::new_with_fake_time().unwrap();
        let test_map = TestMap::new(zx::Duration::from_seconds(10));

        test_map.insert("my_test".into(), "my_test_url".into());
        assert_eq!(test_map.get("my_test"), Some("my_test_url".into()));
        assert_eq!(test_map.get("my_non_existent_test"), None);

        // entry should not be deleted until it is marked as stale.
        executor.set_fake_time(executor.now().add(12.seconds()));
        executor.wake_next_timer();
        let fut = dummy_fn();
        pin_mut!(fut);
        let _poll = executor.run_until_stalled(&mut fut);
        assert_eq!(test_map.get("my_test"), Some("my_test_url".into()));

        // only entry which was marked as stale should be deleted.
        test_map.insert("other_test".into(), "other_test_url".into());
        test_map.mark_as_stale("my_test");
        executor.set_fake_time(executor.now().add(12.seconds()));
        executor.wake_next_timer();
        let fut = dummy_fn();
        pin_mut!(fut);
        let _poll = executor.run_until_stalled(&mut fut);
        assert_eq!(test_map.get("my_test"), None);
        assert_eq!(test_map.get("other_test"), Some("other_test_url".into()));

        // entry should stay in cache for 10 seconds after marking it as stale.
        executor.set_fake_time(executor.now().add(5.seconds()));
        test_map.mark_as_stale("other_test");
        executor.set_fake_time(executor.now().add(5.seconds()));
        executor.wake_next_timer();
        let fut = dummy_fn();
        pin_mut!(fut);
        let _poll = executor.run_until_stalled(&mut fut);
        assert_eq!(test_map.get("other_test"), Some("other_test_url".into()));

        // It has been marked as stale for 10 sec now, so can be deleted.
        executor.set_fake_time(executor.now().add(11.seconds()));
        executor.wake_next_timer();
        let fut = dummy_fn();
        pin_mut!(fut);
        let _poll = executor.run_until_stalled(&mut fut);
        assert_eq!(test_map.get("other_test"), None);
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_map_delete() {
        let test_map = TestMap::new(zx::Duration::from_seconds(10));
        test_map.insert("my_test".into(), "my_test_url".into());
        assert_eq!(test_map.get("my_test"), Some("my_test_url".into()));
        test_map.insert("other_test".into(), "other_test_url".into());
        test_map.delete("my_test");
        assert_eq!(test_map.get("my_test"), None);
        assert_eq!(test_map.get("other_test"), Some("other_test_url".into()));
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn suite_controller_stop_test() {
        let (sender, recv) = mpsc::channel(1024);
        let (stop_sender, stop_recv) = oneshot::channel::<()>();
        let task = fasync::Task::spawn(async move {
            stop_recv.await.unwrap();
            // drop event sender so that fake test can end.
            drop(sender);
        });
        let (proxy, controller) =
            create_proxy_and_stream::<ftest_manager::SuiteControllerMarker>().unwrap();
        let run_controller = fasync::Task::spawn(Suite::run_controller(
            controller,
            stop_sender,
            recv,
            Some(task),
            None,
        ));
        proxy.stop().unwrap();

        assert_eq!(proxy.get_events().await.unwrap(), Ok(vec![]));
        // this should end
        run_controller.await.unwrap();
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn suite_controller_get_events() {
        let (mut sender, recv) = mpsc::channel(1024);
        let (stop_sender, stop_recv) = oneshot::channel::<()>();
        let task = fasync::Task::spawn(async move {});
        let (proxy, controller) =
            create_proxy_and_stream::<ftest_manager::SuiteControllerMarker>().unwrap();
        sender.send(Ok(SuiteEvents::case_found(1, "case1".to_string()).into())).await.unwrap();
        sender.send(Ok(SuiteEvents::case_found(2, "case2".to_string()).into())).await.unwrap();

        let run_controller = fasync::Task::spawn(Suite::run_controller(
            controller,
            stop_sender,
            recv,
            Some(task),
            None,
        ));
        let events = proxy.get_events().await.unwrap().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].payload,
            SuiteEvents::case_found(1, "case1".to_string()).into_suite_run_event().payload,
        );
        assert_eq!(
            events[1].payload,
            SuiteEvents::case_found(2, "case2".to_string()).into_suite_run_event().payload,
        );
        sender.send(Ok(SuiteEvents::case_started(2).into())).await.unwrap();
        proxy.stop().unwrap();

        // test that controller collects event after stop is called.
        sender.send(Ok(SuiteEvents::case_started(1).into())).await.unwrap();
        sender.send(Ok(SuiteEvents::case_found(3, "case3".to_string()).into())).await.unwrap();

        stop_recv.await.unwrap();
        // drop event sender so that fake test can end.
        drop(sender);
        let events = proxy.get_events().await.unwrap().unwrap();
        assert_eq!(events.len(), 3);

        assert_eq!(events[0].payload, SuiteEvents::case_started(2).into_suite_run_event().payload,);
        assert_eq!(events[1].payload, SuiteEvents::case_started(1).into_suite_run_event().payload,);
        assert_eq!(
            events[2].payload,
            SuiteEvents::case_found(3, "case3".to_string()).into_suite_run_event().payload,
        );

        let events = proxy.get_events().await.unwrap().unwrap();
        assert_eq!(events, vec![]);
        // this should end
        run_controller.await.unwrap();
    }

    #[test]
    fn suite_controller_hanging_get_events() {
        let mut executor = fasync::TestExecutor::new().unwrap();
        let (mut sender, recv) = mpsc::channel(1024);
        let (stop_sender, _stop_recv) = oneshot::channel::<()>();
        let task = fasync::Task::spawn(async move {});
        let (proxy, controller) =
            create_proxy_and_stream::<ftest_manager::SuiteControllerMarker>().unwrap();

        let _run_controller = fasync::Task::spawn(Suite::run_controller(
            controller,
            stop_sender,
            recv,
            Some(task),
            None,
        ));

        // send get event call which would hang as there are no events.
        let mut get_events =
            fasync::Task::spawn(async move { proxy.get_events().await.unwrap().unwrap() });
        assert_eq!(executor.run_until_stalled(&mut get_events), std::task::Poll::Pending);
        executor.run_singlethreaded(async {
            sender.send(Ok(SuiteEvents::case_found(1, "case1".to_string()).into())).await.unwrap();
            sender.send(Ok(SuiteEvents::case_found(2, "case2".to_string()).into())).await.unwrap();
        });
        let events = executor.run_singlethreaded(get_events);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].payload,
            SuiteEvents::case_found(1, "case1".to_string()).into_suite_run_event().payload,
        );
        assert_eq!(
            events[1].payload,
            SuiteEvents::case_found(2, "case2".to_string()).into_suite_run_event().payload,
        );
    }

    #[test]
    fn suite_events() {
        let event = SuiteEvents::case_found(1, "case1".to_string()).into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_eq!(
            event.payload,
            Some(FidlSuiteEventPayload::CaseFound(ftest_manager::CaseFound {
                test_case_name: "case1".into(),
                identifier: 1
            }))
        );

        let event = SuiteEvents::case_started(2).into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_eq!(
            event.payload,
            Some(FidlSuiteEventPayload::CaseStarted(ftest_manager::CaseStarted { identifier: 2 }))
        );

        let event = SuiteEvents::case_stopped(2, CaseStatus::Failed).into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_eq!(
            event.payload,
            Some(FidlSuiteEventPayload::CaseStopped(ftest_manager::CaseStopped {
                identifier: 2,
                status: CaseStatus::Failed
            }))
        );

        let event = SuiteEvents::case_finished(2).into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_eq!(
            event.payload,
            Some(FidlSuiteEventPayload::CaseFinished(ftest_manager::CaseFinished {
                identifier: 2
            }))
        );

        let (sock1, _sock2) = zx::Socket::create(zx::SocketOpts::empty()).unwrap();
        let event = SuiteEvents::case_stdout(2, sock1).into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_matches!(
            event.payload,
            Some(FidlSuiteEventPayload::CaseArtifact(ftest_manager::CaseArtifact {
                identifier: 2,
                artifact: ftest_manager::Artifact::Stdout(_)
            }))
        );

        let event = SuiteEvents::suite_finished(SuiteStatus::Failed).into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_eq!(
            event.payload,
            Some(FidlSuiteEventPayload::SuiteFinished(ftest_manager::SuiteFinished {
                status: SuiteStatus::Failed,
            }))
        );

        let (client_end, _server_end) = fidl::endpoints::create_endpoints().unwrap();
        let event = SuiteEvents::suite_syslog(ftest_manager::Syslog::Archive(client_end))
            .into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_matches!(
            event.payload,
            Some(FidlSuiteEventPayload::SuiteArtifact(ftest_manager::SuiteArtifact {
                artifact: ftest_manager::Artifact::Log(ftest_manager::Syslog::Archive(_)),
            }))
        );

        let (client_end, _server_end) = fidl::endpoints::create_endpoints().unwrap();
        let event = SuiteEvents::suite_syslog(ftest_manager::Syslog::Batch(client_end))
            .into_suite_run_event();
        assert_matches!(event.timestamp, Some(_));
        assert_matches!(
            event.payload,
            Some(FidlSuiteEventPayload::SuiteArtifact(ftest_manager::SuiteArtifact {
                artifact: ftest_manager::Artifact::Log(ftest_manager::Syslog::Batch(_)),
            }))
        );
    }
    #[test]
    fn suite_status() {
        let all_case_status = vec![
            CaseStatus::Error,
            CaseStatus::TimedOut,
            CaseStatus::Failed,
            CaseStatus::Skipped,
            CaseStatus::Passed,
        ];
        for status in &all_case_status {
            assert_eq!(
                get_suite_status_from_case_status(SuiteStatus::InternalError, *status),
                SuiteStatus::InternalError
            );
        }

        for status in &all_case_status {
            let s = get_suite_status_from_case_status(SuiteStatus::TimedOut, *status);
            assert_eq!(s, SuiteStatus::TimedOut);
        }

        for status in &all_case_status {
            let s = get_suite_status_from_case_status(SuiteStatus::DidNotFinish, *status);
            let mut expected = SuiteStatus::DidNotFinish;
            if status == &CaseStatus::TimedOut {
                expected = SuiteStatus::TimedOut;
            }
            assert_eq!(s, expected);
        }

        for status in &all_case_status {
            let s = get_suite_status_from_case_status(SuiteStatus::Failed, *status);
            let mut expected = SuiteStatus::Failed;
            if status == &CaseStatus::TimedOut {
                expected = SuiteStatus::TimedOut;
            }
            assert_eq!(s, expected);
        }

        for status in &all_case_status {
            let s = get_suite_status_from_case_status(SuiteStatus::Passed, *status);
            let mut expected = SuiteStatus::Passed;
            if status == &CaseStatus::Error {
                expected = SuiteStatus::Failed;
            }
            if status == &CaseStatus::TimedOut {
                expected = SuiteStatus::TimedOut;
            }
            if status == &CaseStatus::Failed {
                expected = SuiteStatus::Failed;
            }
            assert_eq!(s, expected);
        }

        let all_suite_status = vec![
            SuiteStatus::Passed,
            SuiteStatus::Failed,
            SuiteStatus::TimedOut,
            SuiteStatus::Stopped,
            SuiteStatus::InternalError,
        ];

        for initial_status in &all_suite_status {
            for status in &all_suite_status {
                let s = concat_suite_status(*initial_status, *status);
                let expected: SuiteStatus;
                if initial_status.into_primitive() > status.into_primitive() {
                    expected = *initial_status;
                } else {
                    expected = *status;
                }

                assert_eq!(s, expected);
            }
        }
    }
}
