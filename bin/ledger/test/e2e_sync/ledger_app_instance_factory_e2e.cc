// Copyright 2017 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include "peridot/bin/ledger/test/ledger_app_instance_factory.h"

#include <thread>

#include "peridot/bin/ledger/callback/synchronous_task.h"
#include "peridot/bin/ledger/convert/convert.h"
#include "peridot/bin/ledger/fidl_helpers/bound_interface_set.h"
#include "peridot/bin/ledger/test/fake_token_provider.h"
#include "gtest/gtest.h"
#include "lib/app/cpp/application_context.h"
#include "lib/app/cpp/connect.h"
#include "lib/fidl/cpp/bindings/binding_set.h"
#include "lib/fsl/socket/strings.h"
#include "lib/fsl/tasks/message_loop.h"
#include "lib/fsl/threading/create_thread.h"
#include "lib/fxl/files/scoped_temp_dir.h"
#include "lib/fxl/functional/make_copyable.h"

namespace test {
namespace {
constexpr fxl::StringView kLedgerName = "AppTests";

class LedgerAppInstanceImpl final
    : public LedgerAppInstanceFactory::LedgerAppInstance {
 public:
  LedgerAppInstanceImpl(
      app::ApplicationControllerPtr controller,
      ledger::FirebaseConfigPtr firebase_config,
      ledger::LedgerRepositoryFactoryPtr ledger_repository_factory,
      fxl::RefPtr<fxl::TaskRunner> services_task_runner);

 private:
  app::ApplicationControllerPtr controller_;
};

LedgerAppInstanceImpl::LedgerAppInstanceImpl(
    app::ApplicationControllerPtr controller,
    ledger::FirebaseConfigPtr firebase_config,
    ledger::LedgerRepositoryFactoryPtr ledger_repository_factory,
    fxl::RefPtr<fxl::TaskRunner> services_task_runner)
    : test::LedgerAppInstanceFactory::LedgerAppInstance(
          std::move(firebase_config),
          convert::ToArray(kLedgerName),
          std::move(ledger_repository_factory),
          std::move(services_task_runner)),
      controller_(std::move(controller)) {}

class LedgerAppInstanceFactoryImpl : public LedgerAppInstanceFactory {
 public:
  LedgerAppInstanceFactoryImpl()
      : application_context_(
            app::ApplicationContext::CreateFromStartupInfoNotChecked()) {}
  ~LedgerAppInstanceFactoryImpl() override;
  void Init();

  void SetServerId(std::string server_id) override;

  std::unique_ptr<LedgerAppInstance> NewLedgerAppInstance() override;

 private:
  std::unique_ptr<app::ApplicationContext> application_context_;

  // Thread used to do services.
  std::thread services_thread_;
  fxl::RefPtr<fxl::TaskRunner> services_task_runner_;
  std::string server_id_;
};

void LedgerAppInstanceFactoryImpl::Init() {
  services_thread_ = fsl::CreateThread(&services_task_runner_);
}

LedgerAppInstanceFactoryImpl::~LedgerAppInstanceFactoryImpl() {
  services_task_runner_->PostTask(
      [] { fsl::MessageLoop::GetCurrent()->QuitNow(); });
  services_thread_.join();
}

void LedgerAppInstanceFactoryImpl::SetServerId(std::string server_id) {
  server_id_ = server_id;
}

std::unique_ptr<LedgerAppInstanceFactory::LedgerAppInstance>
LedgerAppInstanceFactoryImpl::NewLedgerAppInstance() {
  app::ApplicationControllerPtr controller;
  ledger::LedgerRepositoryFactoryPtr repository_factory;
  app::ServiceProviderPtr child_services;
  auto launch_info = app::ApplicationLaunchInfo::New();
  launch_info->url = "ledger";
  launch_info->services = child_services.NewRequest();
  launch_info->arguments.push_back("--no_minfs_wait");
  launch_info->arguments.push_back("--no_persisted_config");
  launch_info->arguments.push_back("--no_statistics_reporting_for_testing");

  application_context_->launcher()->CreateApplication(std::move(launch_info),
                                                      controller.NewRequest());
  app::ConnectToService(child_services.get(), repository_factory.NewRequest());

  ledger::FirebaseConfigPtr firebase_config;
  firebase_config = ledger::FirebaseConfig::New();
  firebase_config->server_id = server_id_;
  firebase_config->api_key = "api-key";

  auto result = std::make_unique<LedgerAppInstanceImpl>(
      std::move(controller), std::move(firebase_config),
      std::move(repository_factory), services_task_runner_);
  return result;
}

}  // namespace

std::unique_ptr<LedgerAppInstanceFactory> GetLedgerAppInstanceFactory() {
  auto factory = std::make_unique<LedgerAppInstanceFactoryImpl>();
  factory->Init();
  return factory;
}

}  // namespace test
