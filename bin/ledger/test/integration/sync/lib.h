// Copyright 2017 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#ifndef APPS_LEDGER_SRC_TEST_INTEGRATION_SYNC_LIB_H_
#define APPS_LEDGER_SRC_TEST_INTEGRATION_SYNC_LIB_H_

#include <functional>
#include <memory>

#include "lib/ledger/fidl/ledger.fidl.h"
#include "peridot/bin/ledger/fidl_helpers/bound_interface_set.h"
#include "peridot/bin/ledger/test/fake_token_provider.h"
#include "peridot/bin/ledger/test/get_ledger.h"
#include "peridot/bin/ledger/test/ledger_app_instance_factory.h"
#include "peridot/bin/ledger/test/test_with_message_loop.h"
#include "gtest/gtest.h"
#include "lib/app/cpp/application_context.h"
#include "lib/fxl/files/scoped_temp_dir.h"
#include "lib/fxl/macros.h"

namespace test {
namespace integration {
namespace sync {

// Base test class for synchronization tests. Other tests should derive from
// this class to use the proper synchronization configuration.
class SyncTest : public ::test::TestWithMessageLoop {
 public:
  SyncTest();
  ~SyncTest() override;

  std::unique_ptr<LedgerAppInstanceFactory::LedgerAppInstance>
  NewLedgerAppInstance();

 protected:
  void SetUp() override;

 private:
  std::unique_ptr<LedgerAppInstanceFactory> app_factory_;
};

void ProcessCommandLine(int argc, char** argv);

}  // namespace sync
}  // namespace integration
}  // namespace test

#endif  // APPS_LEDGER_SRC_TEST_INTEGRATION_SYNC_LIB_H_
