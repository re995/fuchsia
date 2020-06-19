// Copyright 2020 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#ifndef SRC_DEVELOPER_FEEDBACK_FEEDBACK_DATA_SYSTEM_LOG_RECORDER_ENCODING_ENCODER_H_
#define SRC_DEVELOPER_FEEDBACK_FEEDBACK_DATA_SYSTEM_LOG_RECORDER_ENCODING_ENCODER_H_

#include <string>

namespace feedback {

// Encodes data, taking into account what it has already encoded so far until it gets reset.
class Encoder {
 public:
  virtual ~Encoder(){};

  virtual std::string Encode(const std::string& msg) = 0;

  // Resets the state of the encoder. The state can consist of previous data, dictionaries, etc.
  virtual void Reset() = 0;
};

}  // namespace feedback

#endif  // SRC_DEVELOPER_FEEDBACK_FEEDBACK_DATA_SYSTEM_LOG_RECORDER_ENCODING_ENCODER_H_
