// Copyright 2020 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include "src/developer/feedback/feedback_data/system_log_recorder/encoding/identity_decoder.h"

#include "src/lib/fsl/vmo/strings.h"

namespace feedback {

std::string IdentityDecoder::Decode(const fsl::SizedVmo& vmo) {
  std::string output;
  fsl::StringFromVmo(vmo, &output);
  return output;
}

}  // namespace feedback
