// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include "system_instance.h"

#include <lib/fdio/directory.h>
#include <zircon/status.h>
#include <zircon/syscalls/policy.h>

#include "devfs.h"
#include "fdio.h"
#include "src/devices/lib/log/log.h"
#include "system_state_manager.h"

DirectoryFilter::~DirectoryFilter() {
  sync_completion_t done;
  vfs_.Shutdown([&done](zx_status_t status) {
    if (status != ZX_OK) {
      LOGF(ERROR, "Failed to shutdown VFS: %s", zx_status_get_string(status));
    }
    sync_completion_signal(&done);
  });
  sync_completion_wait(&done, ZX_TIME_INFINITE);
}

zx_status_t DirectoryFilter::Initialize(zx::channel forwarding_directory,
                                        fbl::Span<const char*> allow_filter) {
  forwarding_dir_ = std::move(forwarding_directory);
  for (const auto& name : allow_filter) {
    zx_status_t status = root_dir_->AddEntry(
        name, fbl::MakeRefCounted<fs::Service>([this, name](zx::channel request) {
          return fdio_service_connect_at(forwarding_dir_.get(), name, request.release());
        }));
    if (status != ZX_OK) {
      return status;
    }
  }
  return ZX_OK;
}

zx_status_t SystemInstance::CreateDriverHostJob(const zx::job& root_job,
                                                zx::job* driver_host_job_out) {
  zx::job driver_host_job;
  zx_status_t status = zx::job::create(root_job, 0u, &driver_host_job);
  if (status != ZX_OK) {
    LOGF(ERROR, "Unable to create driver_host job: %s", zx_status_get_string(status));
    return status;
  }
  // TODO(fxbug.dev/53125): This currently manually restricts AMBIENT_MARK_VMO_EXEC and NEW_PROCESS
  // since this job is created from the root job. The driver_host job should move to being created
  // from something other than the root job. (Although note that it can't simply be created from
  // driver_manager's own job, because that has timer slack job policy automatically applied by the
  // ELF runner.)
  static const zx_policy_basic_v2_t policy[] = {
      {ZX_POL_BAD_HANDLE, ZX_POL_ACTION_ALLOW_EXCEPTION, ZX_POL_OVERRIDE_DENY},
      {ZX_POL_AMBIENT_MARK_VMO_EXEC, ZX_POL_ACTION_DENY, ZX_POL_OVERRIDE_DENY},
      {ZX_POL_NEW_PROCESS, ZX_POL_ACTION_DENY, ZX_POL_OVERRIDE_DENY}};
  status = driver_host_job.set_policy(ZX_JOB_POL_RELATIVE, ZX_JOB_POL_BASIC_V2, &policy,
                                      std::size(policy));
  if (status != ZX_OK) {
    LOGF(ERROR, "Failed to set driver_host job policy: %s", zx_status_get_string(status));
    return status;
  }
  status = driver_host_job.set_property(ZX_PROP_NAME, "zircon-drivers", 15);
  if (status != ZX_OK) {
    LOGF(ERROR, "Failed to set driver_host job property: %s", zx_status_get_string(status));
    return status;
  }
  *driver_host_job_out = std::move(driver_host_job);
  return ZX_OK;
}

void SystemInstance::InstallDevFsIntoNamespace() {
  fdio_ns_t* ns;
  zx_status_t r;
  r = fdio_ns_get_installed(&ns);
  ZX_ASSERT_MSG(r == ZX_OK, "driver_manager: cannot get namespace: %s\n", zx_status_get_string(r));
  r = fdio_ns_bind(ns, "/dev", CloneFs("dev").release());
  ZX_ASSERT_MSG(r == ZX_OK, "driver_manager: cannot bind /dev to namespace: %s\n",
                zx_status_get_string(r));
}

void SystemInstance::ServiceStarter(Coordinator* coordinator) {
  zx_status_t status = coordinator->RegisterWithPowerManager(CloneFs("dev"));
  if (status != ZX_OK) {
    LOGF(WARNING, "Unable to RegisterWithPowerManager: %d", status);
  }

  coordinator->StartLoadingNonBootDrivers();
}

// TODO(fxbug.dev/34633): DEPRECATED. Do not add new dependencies on the fshost loader service!
zx_status_t SystemInstance::clone_fshost_ldsvc(zx::channel* loader) {
  zx::channel remote;
  zx_status_t status = zx::channel::create(0, loader, &remote);
  if (status != ZX_OK) {
    return status;
  }
  return fdio_service_connect("/svc/fuchsia.fshost.Loader", remote.release());
}

zx::channel SystemInstance::CloneFs(const char* path) {
  if (!strcmp(path, "dev")) {
    return devfs_root_clone();
  }
  zx::channel h0, h1;
  if (zx::channel::create(0, &h0, &h1) != ZX_OK) {
    return zx::channel();
  }
  zx_status_t status = ZX_OK;
  if (!strcmp(path, "svc")) {
    status = fdio_service_connect("/svc", h1.release());
  } else if (!strcmp(path, "driver_host_svc")) {
    status = InitializeDriverHostSvcDir();
    if (status == ZX_OK) {
      status = driver_host_svc_->Serve(std::move(h1));
    }
  } else if (!strncmp(path, "dev/", 4)) {
    zx::unowned_channel fs = devfs_root_borrow();
    path += 4;
    status = fdio_open_at(fs->get(), path, FS_READ_WRITE_DIR_FLAGS, h1.release());
  }
  if (status != ZX_OK) {
    LOGF(ERROR, "CloneFs failed for '%s': %s", path, zx_status_get_string(status));
    return zx::channel();
  }
  return h0;
}

zx_status_t SystemInstance::InitializeDriverHostSvcDir() {
  if (driver_host_svc_) {
    return ZX_OK;
  }
  zx_status_t status = loop_.StartThread("driver_host_svc_loop");
  if (status != ZX_OK) {
    return status;
  }
  driver_host_svc_.emplace(loop_.dispatcher());

  zx::channel incoming_services;
  {
    zx::channel server_side;
    status = zx::channel::create(0, &incoming_services, &server_side);
    if (status != ZX_OK) {
      return status;
    }

    status = fdio_service_connect("/svc", server_side.release());
    if (status != ZX_OK) {
      return status;
    }
  }

  const char* kAllowedServices[] = {
      "fuchsia.logger.LogSink",
      "fuchsia.scheduler.ProfileProvider",
      "fuchsia.tracing.provider.Registry",
  };
  return driver_host_svc_->Initialize(std::move(incoming_services), fbl::Span(kAllowedServices));
}
