// Copyright 2018 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include "src/virtualization/bin/guest/launch.h"

#include <lib/syslog/cpp/macros.h>
#include <zircon/status.h>

#include "src/lib/fxl/strings/string_printf.h"
#include "src/virtualization/bin/guest/serial.h"

zx_status_t handle_launch(int argc, const char** argv, async::Loop* loop,
                          fuchsia::virtualization::GuestConfig cfg,
                          sys::ComponentContext* context) {
  // Create environment.
  fuchsia::virtualization::ManagerPtr manager;
  zx_status_t status = context->svc()->Connect(manager.NewRequest());
  if (status != ZX_OK) {
    printf("Failed to connect to guest manager: %s\n", zx_status_get_string(status));
    return status;
  }
  fuchsia::virtualization::RealmPtr realm;
  manager->Create(argv[0], realm.NewRequest());

  // Launch guest.
  auto url = fxl::StringPrintf("fuchsia-pkg://fuchsia.com/%s#meta/%s.cmx", argv[0], argv[0]);
  fuchsia::virtualization::GuestPtr guest;
  realm->LaunchInstance(url, cpp17::nullopt, std::move(cfg), guest.NewRequest(), [](uint32_t) {});

  // Setup serial console.
  SerialConsole console(loop);
  guest->GetSerial([&console](zx::socket socket) { console.Start(std::move(socket)); });
  guest.set_error_handler([&loop](zx_status_t status) {
    fprintf(stderr, "Connection to guest closed: %s\n", zx_status_get_string(status));
    loop->Quit();
  });

  return loop->Run();
}
