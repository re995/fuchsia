// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include "tools/kazoo/outputs.h"
#include "tools/kazoo/syscall_library.h"
#include "tools/kazoo/test.h"
#include "tools/kazoo/test_ir_test_selection.test.h"

namespace {

TEST(VdsoHeaderOutput, TrickyCases) {
  SyscallLibrary library;
  ASSERT_TRUE(SyscallLibraryLoader::FromJson(k_test_selection, &library));

  StringWriter writer;
  ASSERT_TRUE(VdsoHeaderOutput(library, &writer));

  EXPECT_EQ(writer.Out(),
            R"(// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// WARNING: THIS FILE IS MACHINE GENERATED BY //tools/kazoo. DO NOT EDIT.

__LOCAL extern zx_status_t VDSO_zx_selection_futex_requeue(
    const zx_futex_t* value_ptr,
    uint32_t wake_count,
    zx_futex_t current_value,
    const zx_futex_t* requeue_ptr,
    uint32_t requeue_count,
    zx_handle_t new_requeue_owner ZX_USE_HANDLE) __LEAF_FN;

__LOCAL extern zx_status_t SYSCALL_zx_selection_futex_requeue(
    const zx_futex_t* value_ptr,
    uint32_t wake_count,
    zx_futex_t current_value,
    const zx_futex_t* requeue_ptr,
    uint32_t requeue_count,
    zx_handle_t new_requeue_owner ZX_USE_HANDLE) __LEAF_FN;

__LOCAL extern zx_status_t VDSO_zx_selection_object_wait_one(
    zx_handle_t handle ZX_USE_HANDLE,
    zx_signals_t signals,
    zx_time_t deadline,
    zx_signals_t* observed) __LEAF_FN;

__LOCAL extern zx_status_t SYSCALL_zx_selection_object_wait_one(
    zx_handle_t handle ZX_USE_HANDLE,
    zx_signals_t signals,
    zx_time_t deadline,
    zx_signals_t* observed) __LEAF_FN;

__LOCAL extern zx_status_t VDSO_zx_selection_ktrace_read(
    zx_handle_t handle ZX_USE_HANDLE,
    void* data,
    uint32_t offset,
    size_t data_size,
    size_t* actual) __NONNULL((5)) __LEAF_FN;

__LOCAL extern zx_status_t SYSCALL_zx_selection_ktrace_read(
    zx_handle_t handle ZX_USE_HANDLE,
    void* data,
    uint32_t offset,
    size_t data_size,
    size_t* actual) __NONNULL((5)) __LEAF_FN;

__LOCAL extern zx_status_t VDSO_zx_selection_pci_cfg_pio_rw(
    zx_handle_t handle ZX_USE_HANDLE,
    uint8_t bus,
    uint8_t dev,
    uint8_t func,
    uint8_t offset,
    uint32_t* val,
    size_t width,
    bool write) __LEAF_FN;

__LOCAL extern zx_status_t SYSCALL_zx_selection_pci_cfg_pio_rw(
    zx_handle_t handle ZX_USE_HANDLE,
    uint8_t bus,
    uint8_t dev,
    uint8_t func,
    uint8_t offset,
    uint32_t* val,
    size_t width,
    bool write) __LEAF_FN;

__LOCAL extern zx_status_t VDSO_zx_selection_job_set_policy(
    zx_handle_t handle ZX_USE_HANDLE,
    uint32_t options,
    uint32_t topic,
    const void* policy,
    uint32_t policy_size) __LEAF_FN;

__LOCAL extern zx_status_t SYSCALL_zx_selection_job_set_policy(
    zx_handle_t handle ZX_USE_HANDLE,
    uint32_t options,
    uint32_t topic,
    const void* policy,
    uint32_t policy_size) __LEAF_FN;

__LOCAL extern zx_status_t VDSO_zx_selection_clock_get(
    zx_clock_t clock_id,
    zx_time_t* out) __NONNULL((2)) __LEAF_FN;

__LOCAL extern zx_status_t SYSCALL_zx_selection_clock_get(
    zx_clock_t clock_id,
    zx_time_t* out) __NONNULL((2)) __LEAF_FN;

)");
}

}  // namespace

