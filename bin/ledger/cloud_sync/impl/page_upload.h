// Copyright 2017 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#ifndef APPS_LEDGER_SRC_CLOUD_SYNC_IMPL_PAGE_UPLOAD_H_
#define APPS_LEDGER_SRC_CLOUD_SYNC_IMPL_PAGE_UPLOAD_H_

#include <memory>
#include <vector>

#include "peridot/bin/ledger/cloud_provider/public/page_cloud_handler.h"
#include "peridot/bin/ledger/cloud_sync/impl/base_coordinator_delegate.h"
#include "peridot/bin/ledger/cloud_sync/impl/batch_upload.h"
#include "peridot/bin/ledger/cloud_sync/public/sync_state_watcher.h"
#include "peridot/bin/ledger/storage/public/commit.h"
#include "peridot/bin/ledger/storage/public/commit_watcher.h"
#include "peridot/bin/ledger/storage/public/types.h"

namespace cloud_sync {
// PageUpload handles all the upload operations for a page.
class PageUpload : public storage::CommitWatcher {
 public:
  // Delegate ensuring coordination between PageUpload and the class that owns
  // it.
  class Delegate : public BaseCoordinatorDelegate {
   public:
    // Reports that the upload state changed.
    virtual void SetUploadState(UploadSyncState sync_state) = 0;

    // Returns true if no download is in progress.
    virtual bool IsDownloadIdle() = 0;
  };

  PageUpload(storage::PageStorage* storage,
             cloud_provider_firebase::PageCloudHandler* cloud_provider,
             auth_provider::AuthProvider* auth_provider,
             Delegate* delegate);

  ~PageUpload() override;

  // Uploads the initial backlog of local unsynced commits, and sets up the
  // storage watcher upon success.
  void StartUpload();

  // Returns true if PageUpload is idle.
  bool IsIdle();

 private:
  // storage::CommitWatcher:
  void OnNewCommits(
      const std::vector<std::unique_ptr<const storage::Commit>>& /*commits*/,
      storage::ChangeSource source) override;

  void UploadUnsyncedCommits();
  void VerifyUnsyncedCommits(
      std::vector<std::unique_ptr<const storage::Commit>> commits);
  void HandleUnsyncedCommits(
      std::vector<std::unique_ptr<const storage::Commit>> commits);

  // Sets the internal state.
  void SetState(UploadSyncState new_state);

  void HandleError(const char error_description[]);

  storage::PageStorage* const storage_;
  cloud_provider_firebase::PageCloudHandler* const cloud_provider_;
  auth_provider::AuthProvider* const auth_provider_;
  Delegate* const delegate_;
  const std::string log_prefix_;

  // Work queue:
  // Current batch of local commits being uploaded.
  std::unique_ptr<BatchUpload> batch_upload_;
  // Set to true when there are new commits to upload.
  bool commits_to_upload_ = false;

  // Internal state.
  UploadSyncState state_ = UPLOAD_STOPPED;

  FXL_DISALLOW_COPY_AND_ASSIGN(PageUpload);
};

}  // namespace cloud_sync

#endif  // APPS_LEDGER_SRC_CLOUD_SYNC_IMPL_PAGE_UPLOAD_H_
