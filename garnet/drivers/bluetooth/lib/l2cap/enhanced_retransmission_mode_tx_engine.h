// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#ifndef GARNET_DRIVERS_BLUETOOTH_LIB_L2CAP_ENHANCED_RETRANSMISSION_MODE_TX_ENGINE_H_
#define GARNET_DRIVERS_BLUETOOTH_LIB_L2CAP_ENHANCED_RETRANSMISSION_MODE_TX_ENGINE_H_

#include "garnet/drivers/bluetooth/lib/l2cap/tx_engine.h"

namespace btlib {
namespace l2cap {
namespace internal {

// Implements the sender-side functionality of L2CAP Enhanced Retransmission
// Mode. See Bluetooth Core Spec v5.0, Volume 3, Part A, Sec 2.4, "Modes of
// Operation".
//
// THREAD-SAFETY: This class may is _not_ thread-safe. In particular, the class
// assumes that some other party ensures that QueueSdu() is not invoked
// concurrently with the destructor.
class EnhancedRetransmissionModeTxEngine final : public TxEngine {
 public:
  EnhancedRetransmissionModeTxEngine(
      ChannelId channel_id, uint16_t tx_mtu,
      SendBasicFrameCallback send_basic_frame_callback)
      : TxEngine(channel_id, tx_mtu, std::move(send_basic_frame_callback)) {}
  ~EnhancedRetransmissionModeTxEngine() override = default;

  bool QueueSdu(common::ByteBufferPtr sdu) override;

 private:
  FXL_DISALLOW_COPY_AND_ASSIGN(EnhancedRetransmissionModeTxEngine);
};

}  // namespace internal
}  // namespace l2cap
}  // namespace btlib

#endif  // GARNET_DRIVERS_BLUETOOTH_LIB_L2CAP_ENHANCED_RETRANSMISSION_MODE_TX_ENGINE_H_
