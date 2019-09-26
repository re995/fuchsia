// Copyright 2018 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#ifndef SRC_DEVELOPER_DEBUG_ZXDB_CLIENT_THREAD_CONTROLLER_TEST_H_
#define SRC_DEVELOPER_DEBUG_ZXDB_CLIENT_THREAD_CONTROLLER_TEST_H_

#include <memory>

#include "src/developer/debug/zxdb/client/mock_remote_api.h"
#include "src/developer/debug/zxdb/client/remote_api_test.h"
#include "src/developer/debug/zxdb/symbols/mock_module_symbols.h"
#include "src/developer/debug/zxdb/symbols/system_symbols.h"

namespace zxdb {

class Process;
class Thread;

// This test harness automatically makes a process and a thread
//
// In the future we will probably want to add support for setting up a mock symbol system (this is
// more involved).
class ThreadControllerTest : public RemoteAPITest {
 public:
  ThreadControllerTest();
  ~ThreadControllerTest();

  void SetUp() override;

  Process* process() { return process_; }
  Thread* thread() { return thread_; }

  // Load address that a mock module with no symbols is loaded at. If a test needs an address into
  // an unsymbolized module, it should be between this value and kSymbolizedModuleAddress.
  static const uint64_t kUnsymbolizedModuleAddress;

  // Load address that the mock module with symbols is loaded at. Addresses you want to support
  // symbol lookup for need to be larger than this.
  static const uint64_t kSymbolizedModuleAddress;

  // The mock module symbols. Addresses above kModuleAddress will be handled by this mock. Test code
  // should inject the responses it wants into this mock.
  MockModuleSymbols* module_symbols() const { return module_symbols_.get(); }

 private:
  // Non-owning pointer to the injected fake process/thread.
  Process* process_ = nullptr;
  Thread* thread_ = nullptr;

  fxl::RefPtr<MockModuleSymbols> module_symbols_;
};

}  // namespace zxdb

#endif  // SRC_DEVELOPER_DEBUG_ZXDB_CLIENT_THREAD_CONTROLLER_TEST_H_
