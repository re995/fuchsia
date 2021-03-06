// Copyright 2021 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
use {
    anyhow::Result,
    async_lock::RwLock,
    async_trait::async_trait,
    ffx_config::{self, ConfigLevel},
    fidl::endpoints::create_proxy,
    fidl_fuchsia_developer_bridge as bridge,
    fidl_fuchsia_developer_bridge_ext::RepositorySpec,
    fidl_fuchsia_pkg::RepositoryManagerMarker,
    fidl_fuchsia_pkg_rewrite::{EngineMarker, LiteralRule, Rule},
    fuchsia_async::{self as fasync, futures::StreamExt as _},
    pkg::repository::{Repository, RepositoryManager, RepositoryServer, LISTEN_PORT},
    serde_json,
    services::prelude::*,
    std::{convert::TryInto, net, sync::Arc},
};

const REPOSITORY_MANAGER_SELECTOR: &str = "core/appmgr:out:fuchsia.pkg.RepositoryManager";
const REWRITE_SERVICE_SELECTOR: &str = "core/appmgr:out:fuchsia.pkg.rewrite.Engine";

struct ServerInfo {
    server: RepositoryServer,
    addr: net::SocketAddr,
    task: fasync::Task<()>,
}

#[ffx_service]
pub struct Repo {
    manager: Arc<RepositoryManager>,
    server: RwLock<Option<ServerInfo>>,
}

impl Repo {
    async fn start_server(&self, addr: net::SocketAddr) -> bool {
        let mut server_locked = self.server.write().await;
        match &*server_locked {
            Some(x) if x.addr == addr => true,
            _ => {
                *server_locked = None;

                log::info!("Trying to serve on {}", addr);
                if let Err(x) = ffx_config::set(
                    ("repository_server.listen", ConfigLevel::User),
                    format!("{}", addr).into(),
                )
                .await
                {
                    log::warn!("Could not save address to config: {}", x);
                }

                match RepositoryServer::builder(addr, Arc::clone(&self.manager)).start().await {
                    Ok((task, server)) => {
                        let task = fasync::Task::local(task);
                        log::info!("Started server on {}", server.local_addr());
                        *server_locked = Some(ServerInfo { server, addr, task });
                        true
                    }
                    Err(x) => {
                        log::warn!("Could not start server: {}", x);
                        false
                    }
                }
            }
        }
    }

    async fn add_repository(
        &self,
        name: String,
        repo_spec: bridge::RepositorySpec,
    ) -> std::result::Result<(), bridge::RepositoryError> {
        log::info!("Adding repository {} {:?}", name, repo_spec);

        let repo_spec: RepositorySpec = repo_spec.try_into()?;

        let json_repo_spec = serde_json::to_value(repo_spec.clone()).map_err(|err| {
            log::error!("unable to serialize repository spec {:?}: {}", repo_spec, err);
            bridge::RepositoryError::InternalError
        })?;

        // Create the repository.
        let repo = Repository::from_repository_spec(&name, repo_spec).await.map_err(|err| {
            log::error!("unable to create repository: {}", err);
            bridge::RepositoryError::IoError
        })?;

        // Save the filesystem configuration.
        ffx_config::set(
            (format!("repository_server.repositories.{}", name).as_str(), ConfigLevel::User),
            json_repo_spec,
        )
        .await
        .map_err(|err| {
            log::error!("failed to save repository: {}", err);
            bridge::RepositoryError::IoError
        })?;

        // Finally add the repository.
        self.manager.add(Arc::new(repo));

        Ok(())
    }

    async fn register_target(
        &self,
        cx: &Context,
        target_info: bridge::RepositoryTarget,
    ) -> std::result::Result<(), bridge::RepositoryError> {
        let repo_name = target_info.repo_name.as_ref().ok_or_else(|| {
            log::warn!("Registering repository target is missing a repository name");
            bridge::RepositoryError::MissingRepositoryName
        })?;

        log::info!(
            "Registering repository {:?} for target {:?}",
            repo_name,
            target_info.target_identifier
        );

        let repo = self
            .manager
            .get(&repo_name)
            .ok_or_else(|| bridge::RepositoryError::NoMatchingRepository)?;

        let proxy = cx
            .open_target_proxy::<RepositoryManagerMarker>(
                target_info.target_identifier.clone(),
                REPOSITORY_MANAGER_SELECTOR,
            )
            .await
            .map_err(|err| {
                log::warn!(
                    "failed to open target proxy with target name {:?}: {}",
                    target_info.target_identifier,
                    err
                );
                bridge::RepositoryError::TargetCommunicationFailure
            })?;

        // TODO(fxbug.dev/77015): parameterize the mirror_url value here once we are dynamically assigning ports.
        let config = repo.get_config(&format!("localhost:{}/{}", LISTEN_PORT, repo.name()));

        match proxy.add(config.into()).await {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                log::warn!("failed to add config: {}", err);
                return Err(bridge::RepositoryError::RepositoryManagerError);
            }
            Err(err) => {
                log::warn!("failed to add config: {}", err);
                return Err(bridge::RepositoryError::TargetCommunicationFailure);
            }
        }

        let aliases = target_info.aliases.unwrap_or(vec![]);
        if aliases.is_empty() {
            return Ok(());
        }

        let rewrite_proxy = match cx
            .open_target_proxy::<EngineMarker>(
                target_info.target_identifier.clone(),
                REWRITE_SERVICE_SELECTOR,
            )
            .await
        {
            Ok(p) => p,
            Err(e) => {
                log::warn!(
                    "failed to open Rewrite Engine target proxy with target name {:?}: {}",
                    target_info.target_identifier,
                    e
                );
                return Err(bridge::RepositoryError::TargetCommunicationFailure);
            }
        };

        let (transaction_proxy, server_end) = create_proxy().map_err(|err| {
            log::warn!("failed to create Rewrite transaction: {}", err);
            bridge::RepositoryError::RewriteEngineError
        })?;
        rewrite_proxy.start_edit_transaction(server_end).map_err(|err| {
            log::warn!("failed to start edit transaction: {}", err);
            bridge::RepositoryError::RewriteEngineError
        })?;

        for alias in aliases.iter() {
            let rule = LiteralRule {
                host_match: alias.to_string(),
                host_replacement: repo.name().to_string(),
                path_prefix_match: "/".to_string(),
                path_prefix_replacement: "/".to_string(),
            };
            match transaction_proxy.add(&mut Rule::Literal(rule)).await {
                Err(e) => {
                    log::warn!("failed to add rewrite rule. Error was: {}", e);
                    return Err(bridge::RepositoryError::RewriteEngineError);
                }
                Ok(Err(e)) => {
                    log::warn!("Adding rewrite rule returned failure. Error was: {}", e);
                    return Err(bridge::RepositoryError::RewriteEngineError);
                }
                Ok(_) => {}
            }
        }

        match transaction_proxy.commit().await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                log::warn!("Committing rewrite rule returned failure. Error was: {}", e);
                Err(bridge::RepositoryError::RewriteEngineError)
            }
            Err(e) => {
                log::warn!("failed to commit rewrite rule. Error was: {}", e);
                Err(bridge::RepositoryError::RewriteEngineError)
            }
        }
    }
}

impl Default for Repo {
    fn default() -> Self {
        Repo { manager: RepositoryManager::new(), server: RwLock::new(None) }
    }
}

#[async_trait(?Send)]
impl FidlService for Repo {
    type Service = bridge::RepositoryRegistryMarker;
    type StreamHandler = FidlStreamHandler<Self>;

    async fn handle(&self, cx: &Context, req: bridge::RepositoryRegistryRequest) -> Result<()> {
        match req {
            bridge::RepositoryRegistryRequest::AddRepository { name, repository, responder } => {
                responder.send(&mut self.add_repository(name, repository).await)?;
                Ok(())
            }
            bridge::RepositoryRegistryRequest::RemoveRepository { name, responder } => {
                log::info!("Removing repository {}", name);

                responder.send(self.manager.remove(&name))?;
                Ok(())
            }
            bridge::RepositoryRegistryRequest::RegisterTarget { target_info, responder } => {
                responder.send(&mut self.register_target(cx, target_info).await)?;
                Ok(())
            }
            bridge::RepositoryRegistryRequest::ListRepositories { iterator, .. } => {
                let mut stream = iterator.into_stream()?;
                let mut values = self
                    .manager
                    .repositories()
                    .map(|x| bridge::RepositoryConfig {
                        name: x.name().to_owned(),
                        spec: x.spec().into(),
                    })
                    .collect::<Vec<_>>();
                let mut pos = 0;

                fasync::Task::spawn(async move {
                    while let Some(request) = stream.next().await {
                        match request {
                            Ok(bridge::RepositoryIteratorRequest::Next { responder }) => {
                                let len = values.len();
                                let chunk = &mut values[pos..]
                                    [..std::cmp::min(len - pos, bridge::MAX_REPOS as usize)]
                                    .iter_mut();
                                pos += bridge::MAX_REPOS as usize;
                                pos = std::cmp::min(pos, len);

                                if let Err(e) = responder.send(chunk) {
                                    log::warn!(
                                        "Error responding to RepositoryIterator request: {}",
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                log::warn!("Error in RepositoryIterator request stream: {}", e)
                            }
                        }
                    }
                })
                .detach();
                Ok(())
            }
        }
    }

    async fn start(&mut self, _cx: &Context) -> Result<()> {
        log::info!("started repository service");

        if let Ok(address) = ffx_config::get::<String, _>("repository_server.listen").await {
            if let Ok(address) = address.parse::<net::SocketAddr>() {
                self.start_server(address).await;
            } else {
                log::warn!("Invalid value for repository_server.listen")
            }
        }

        if let Ok(repos) = ffx_config::get::<serde_json::Map<String, serde_json::Value>, _>(
            "repository_server.repositories",
        )
        .await
        {
            for (name, entry) in repos.into_iter() {
                let repo_spec = match serde_json::from_value(entry) {
                    Ok(repo_spec) => repo_spec,
                    Err(err) => {
                        log::warn!("failed to parse repository {:?}: {}", name, err);
                        continue;
                    }
                };

                let repo = match Repository::from_repository_spec(&name, repo_spec).await {
                    Ok(repo) => repo,
                    Err(err) => {
                        log::warn!("failed to create repository {:?}: {}", name, err);
                        continue;
                    }
                };

                self.manager.add(Arc::new(repo));
            }
        }

        Ok(())
    }

    async fn stop(&mut self, _cx: &Context) -> Result<()> {
        let server_info = self.server.write().await.take();
        if let Some(server_info) = server_info {
            server_info.server.stop();

            // TODO(sadmac): Timeout.
            server_info.task.await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        fidl,
        fidl_fuchsia_pkg::{
            MirrorConfig, RepositoryConfig, RepositoryKeyConfig, RepositoryManagerRequest,
        },
        fidl_fuchsia_pkg_rewrite::{EditTransactionRequest, EngineMarker, EngineRequest},
        services::testing::{FakeDaemon, FakeDaemonBuilder},
    };

    const REPO_NAME: &str = "some_repo";
    const EMPTY_REPO_PATH: &str = "host_x64/test_data/ffx_daemon_service_repo/empty-repo";

    #[derive(Default)]
    struct FakeRepositoryManager {}

    fn test_repo_config() -> RepositoryConfig {
        RepositoryConfig {
            mirrors: Some(vec![MirrorConfig {
                mirror_url: Some(format!("http://localhost:8085/{}", REPO_NAME)),
                subscribe: Some(true),
                ..MirrorConfig::EMPTY
            }]),
            repo_url: Some(format!("fuchsia-pkg://{}", REPO_NAME)),
            root_keys: Some(vec![RepositoryKeyConfig::Ed25519Key(vec![
                29, 76, 86, 76, 184, 70, 108, 73, 249, 127, 4, 47, 95, 63, 36, 35, 101, 255, 212,
                33, 10, 154, 26, 130, 117, 157, 125, 88, 175, 214, 109, 113,
            ])]),
            root_version: Some(1),
            ..RepositoryConfig::EMPTY
        }
    }

    #[async_trait(?Send)]
    impl FidlService for FakeRepositoryManager {
        type Service = RepositoryManagerMarker;
        type StreamHandler = FidlStreamHandler<Self>;

        async fn handle(&self, _cx: &Context, req: RepositoryManagerRequest) -> Result<()> {
            match req {
                RepositoryManagerRequest::Add { repo, responder } => {
                    assert_eq!(repo, test_repo_config(),);
                    responder.send(&mut Ok(())).unwrap()
                }
                _ => {
                    panic!("unexpected RepositoryManager request {:?}", req);
                }
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct ErroringRepositoryManager {}

    #[async_trait(?Send)]
    impl FidlService for ErroringRepositoryManager {
        type Service = RepositoryManagerMarker;
        type StreamHandler = FidlStreamHandler<Self>;

        async fn handle(&self, _cx: &Context, req: RepositoryManagerRequest) -> Result<()> {
            match req {
                RepositoryManagerRequest::Add { repo, responder } => {
                    assert_eq!(repo, test_repo_config(),);

                    responder.send(&mut Err(1)).unwrap()
                }
                _ => {
                    panic!("unexpected RepositoryManager request {:?}", req);
                }
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeEngine {}
    #[async_trait(?Send)]
    impl FidlService for FakeEngine {
        type Service = EngineMarker;
        type StreamHandler = FidlStreamHandler<Self>;

        async fn handle(&self, _cx: &Context, req: EngineRequest) -> Result<()> {
            match req {
                EngineRequest::StartEditTransaction { transaction, .. } => {
                    let mut stream = transaction.into_stream().unwrap();
                    while let Some(request) = stream.next().await {
                        let request = request.unwrap();
                        match request {
                            EditTransactionRequest::Add { rule, responder } => {
                                assert_eq!(
                                    rule,
                                    Rule::Literal(LiteralRule {
                                        host_match: "fuchsia.com".to_string(),
                                        host_replacement: REPO_NAME.to_string(),
                                        path_prefix_match: "/".to_string(),
                                        path_prefix_replacement: "/".to_string(),
                                    })
                                );
                                responder.send(&mut Ok(())).unwrap()
                            }

                            EditTransactionRequest::Commit { responder } => {
                                responder.send(&mut Ok(())).unwrap()
                            }
                            _ => {
                                panic!("unexpected EditTransaction request");
                            }
                        }
                    }
                }
                _ => {
                    panic!("unexpected Engine request {:?}", req);
                }
            }
            Ok(())
        }
    }

    async fn add_repo(daemon: &FakeDaemon) -> bridge::RepositoryRegistryProxy {
        let proxy = daemon.open_proxy::<bridge::RepositoryRegistryMarker>().await;
        let spec = RepositorySpec::FileSystem { path: EMPTY_REPO_PATH.into() };
        proxy
            .add_repository(REPO_NAME, &mut spec.into())
            .await
            .expect("communicated with proxy")
            .expect("adding repository to succeed");

        proxy
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_add_remove() {
        ffx_config::init_config_test().unwrap();

        let daemon = FakeDaemonBuilder::new().register_fidl_service::<Repo>().build();

        let proxy = daemon.open_proxy::<bridge::RepositoryRegistryMarker>().await;
        let spec = bridge::RepositorySpec::FileSystem(bridge::FileSystemRepositorySpec {
            path: Some(EMPTY_REPO_PATH.to_owned()),
            ..bridge::FileSystemRepositorySpec::EMPTY
        });
        proxy
            .add_repository(REPO_NAME, &mut spec.clone())
            .await
            .expect("communicated with proxy")
            .expect("adding repository to succeed");

        let (client, server) = fidl::endpoints::create_endpoints().unwrap();
        proxy.list_repositories(server).unwrap();
        let client = client.into_proxy().unwrap();

        let next = client.next().await.unwrap();

        assert_eq!(1, next.len());
        assert_eq!(0, client.next().await.unwrap().len());

        let got = &next[0];
        assert_eq!(REPO_NAME, &got.name);
        assert_eq!(spec, got.spec);

        assert!(proxy.remove_repository(REPO_NAME).await.unwrap());

        let (client, server) = fidl::endpoints::create_endpoints().unwrap();
        proxy.list_repositories(server).unwrap();
        let client = client.into_proxy().unwrap();

        assert_eq!(0, client.next().await.unwrap().len());
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_add_register() {
        ffx_config::init_config_test().unwrap();

        let daemon = FakeDaemonBuilder::new()
            .register_fidl_service::<FakeRepositoryManager>()
            .register_fidl_service::<FakeEngine>()
            .register_fidl_service::<Repo>()
            .build();

        let proxy = add_repo(&daemon).await;
        proxy
            .register_target(bridge::RepositoryTarget {
                repo_name: Some(REPO_NAME.to_string()),
                target_identifier: None,
                aliases: Some(vec!["fuchsia.com".to_string()]),
                ..bridge::RepositoryTarget::EMPTY
            })
            .await
            .expect("communicated with proxy")
            .expect("target registration to succeed");
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_add_register_empty_aliases() {
        ffx_config::init_config_test().unwrap();

        let daemon = FakeDaemonBuilder::new()
            .register_fidl_service::<FakeRepositoryManager>()
            .register_fidl_service::<FakeEngine>()
            .register_fidl_service::<Repo>()
            .build();

        let proxy = add_repo(&daemon).await;
        proxy
            .register_target(bridge::RepositoryTarget {
                repo_name: Some(REPO_NAME.to_string()),
                target_identifier: None,
                aliases: Some(vec![]),
                ..bridge::RepositoryTarget::EMPTY
            })
            .await
            .unwrap()
            .unwrap();
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_add_register_none_aliases() {
        ffx_config::init_config_test().unwrap();

        let daemon = FakeDaemonBuilder::new()
            .register_fidl_service::<FakeRepositoryManager>()
            .register_fidl_service::<FakeEngine>()
            .register_fidl_service::<Repo>()
            .build();

        let proxy = add_repo(&daemon).await;
        proxy
            .register_target(bridge::RepositoryTarget {
                repo_name: Some(REPO_NAME.to_string()),
                target_identifier: None,
                aliases: None,
                ..bridge::RepositoryTarget::EMPTY
            })
            .await
            .unwrap()
            .unwrap();
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_add_register_repo_manager_error() {
        ffx_config::init_config_test().unwrap();

        let daemon = FakeDaemonBuilder::new()
            .register_fidl_service::<ErroringRepositoryManager>()
            .register_fidl_service::<FakeEngine>()
            .register_fidl_service::<Repo>()
            .build();

        let proxy = add_repo(&daemon).await;
        assert_eq!(
            proxy
                .register_target(bridge::RepositoryTarget {
                    repo_name: Some(REPO_NAME.to_string()),
                    target_identifier: None,
                    aliases: None,
                    ..bridge::RepositoryTarget::EMPTY
                })
                .await
                .unwrap()
                .unwrap_err(),
            bridge::RepositoryError::RepositoryManagerError
        );
    }

    #[fuchsia_async::run_singlethreaded(test)]
    async fn test_register_non_existent_repo() {
        let daemon = FakeDaemonBuilder::new()
            .register_fidl_service::<ErroringRepositoryManager>()
            .register_fidl_service::<FakeEngine>()
            .register_fidl_service::<Repo>()
            .build();

        let proxy = daemon.open_proxy::<bridge::RepositoryRegistryMarker>().await;
        assert_eq!(
            proxy
                .register_target(bridge::RepositoryTarget {
                    repo_name: Some(REPO_NAME.to_string()),
                    target_identifier: None,
                    aliases: None,
                    ..bridge::RepositoryTarget::EMPTY
                })
                .await
                .unwrap()
                .unwrap_err(),
            bridge::RepositoryError::NoMatchingRepository
        );
    }
}
