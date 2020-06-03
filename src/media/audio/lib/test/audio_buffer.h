// Copyright 2020 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#ifndef SRC_MEDIA_AUDIO_LIB_TEST_AUDIO_BUFFER_H_
#define SRC_MEDIA_AUDIO_LIB_TEST_AUDIO_BUFFER_H_

#include <fuchsia/media/cpp/fidl.h>
#include <fuchsia/virtualaudio/cpp/fidl.h>
#include <lib/fzl/vmo-mapper.h>
#include <lib/zx/vmo.h>

#include <cmath>
#include <memory>

#include "src/lib/fxl/strings/string_printf.h"
#include "src/media/audio/lib/format/constants.h"
#include "src/media/audio/lib/format/format.h"
#include "src/media/audio/lib/format/traits.h"
#include "src/media/audio/lib/test/hermetic_audio_environment.h"
#include "src/media/audio/lib/wav/wav_reader.h"

namespace media::audio::test {

// A buffer of audio data. Each entry in the vector is a single sample.
template <fuchsia::media::AudioSampleFormat SampleFormat>
struct AudioBuffer {
  using SampleT = typename SampleFormatTraits<SampleFormat>::SampleT;

  Format format;
  std::vector<SampleT> samples;

  AudioBuffer(const Format& f, size_t num_frames) : format(f), samples(num_frames * f.channels()) {
    FX_CHECK(sizeof(SampleT) == f.bytes_per_frame() / f.channels());
  }

  size_t NumFrames() const { return samples.size() / format.channels(); }
  size_t NumBytes() const { return NumFrames() * format.bytes_per_frame(); }
  size_t SampleIndex(size_t frame, size_t chan) const { return frame * format.channels() + chan; }
  SampleT SampleAt(size_t frame, size_t chan) const { return samples[SampleIndex(frame, chan)]; }

  // For debugging, display the given range of frames.
  void Display(size_t start_frame, size_t end_frame) const {
    printf("\n\n Frames %zu to %zu: ", start_frame, end_frame);
    for (auto frame = start_frame; frame < end_frame; ++frame) {
      if (frame % 16 == 0) {
        printf("\n [%6lu] ", frame);
      } else {
        printf(" ");
      }
      for (auto chan = 0u; chan < format.channels(); ++chan) {
        auto offset = frame * format.channels() + chan;
        printf("%s", SampleFormatTraits<SampleFormat>::ToString(samples[offset]).c_str());
      }
    }
    printf("\n");
  }
};

// A slice of an AudioBuffer.
template <fuchsia::media::AudioSampleFormat SampleFormat>
struct AudioBufferSlice {
  using SampleT = typename SampleFormatTraits<SampleFormat>::SampleT;

  const AudioBuffer<SampleFormat>* buf;
  size_t start_frame;
  size_t end_frame;

  AudioBufferSlice() : buf(nullptr), start_frame(0), end_frame(0) {}
  AudioBufferSlice(const AudioBuffer<SampleFormat>* b)
      : buf(b), start_frame(0), end_frame(b->NumFrames()) {}
  AudioBufferSlice(const AudioBuffer<SampleFormat>* b, size_t s, size_t e)
      : buf(b), start_frame(std::min(s, b->NumFrames())), end_frame(std::min(e, b->NumFrames())) {}

  const Format& format() const { return buf->format; }
  size_t NumFrames() const { return end_frame - start_frame; }
  size_t NumBytes() const { return NumFrames() * buf->format.bytes_per_frame(); }
  size_t NumSamples() const { return NumFrames() * buf->format.channels(); }
  size_t SampleIndex(size_t frame, size_t chan) const {
    return buf->SampleIndex(start_frame + frame, chan);
  }
  SampleT SampleAt(size_t frame, size_t chan) const {
    return buf->SampleAt(start_frame + frame, chan);
  }

  // Return a buffer containing the given channel only.
  AudioBuffer<SampleFormat> GetChannel(size_t chan) {
    auto new_format = Format::Create({
                                         .sample_format = SampleFormat,
                                         .channels = 1,
                                         .frames_per_second = format().frames_per_second(),
                                     })
                          .take_value();
    AudioBuffer<SampleFormat> out(new_format, NumFrames());
    for (size_t frame = 0; frame < NumFrames(); frame++) {
      out.buf[frame] = SampleAt(frame, chan);
    }
    return out;
  }
};

// Construct a stream of silent audio data.
template <fuchsia::media::AudioSampleFormat SampleFormat>
AudioBuffer<SampleFormat> GenerateSilentAudio(Format format, size_t num_frames) {
  AudioBuffer<SampleFormat> buf(format, num_frames);
  std::fill(buf.samples.begin(), buf.samples.end(), SampleFormatTraits<SampleFormat>::kSilentValue);
  return buf;
}

// Construct a stream of synthetic audio data that is sequentially incremented. For integer types,
// payload data values increase by 1. For FLOAT, data increases by 2^-16, which is about 10^-5.
//
// As this does not create a meaningful sound, this is intended to be used in test scenarios that
// perform bit-for-bit comparisons on the output of an audio pipeline.
template <fuchsia::media::AudioSampleFormat SampleFormat>
AudioBuffer<SampleFormat> GenerateSequentialAudio(
    Format format, size_t num_frames, typename AudioBuffer<SampleFormat>::SampleT first_val = 0) {
  typename AudioBuffer<SampleFormat>::SampleT increment = 1;
  if (SampleFormat == fuchsia::media::AudioSampleFormat::FLOAT) {
    increment = pow(2.0, -16);
  }
  AudioBuffer<SampleFormat> out(format, num_frames);
  for (size_t sample = 0; sample < out.samples.size(); ++sample) {
    out.samples[sample] = first_val;
    first_val += increment;
    if (SampleFormat == fuchsia::media::AudioSampleFormat::FLOAT && first_val > 1) {
      first_val = -1;
    }
  }
  return out;
}

// Construct a stream of sinusoidal values of the given number of frames, determined by equation
// "buffer[idx] = magn * cosine(idx*freq/num_frames*2*M_PI + phase)". If the format has >1 channels,
// each channel is assigned a duplicate value.
//
// Restated: |freq| is the number of **complete sinusoidal periods** that should perfectly fit into
// the buffer; |magn| is a multiplier applied to the output (default value is 1.0); |phase| is an
// offset (default value 0.0) which shifts the signal along the x-axis (value expressed in radians,
// so runs from -M_PI to +M_PI).
template <fuchsia::media::AudioSampleFormat SampleFormat>
AudioBuffer<SampleFormat> GenerateCosineAudio(Format format, size_t num_frames, double freq,
                                              double magn = 1.0, double phase = 0.0) {
  // If frequency is 0 (constant val), phase offset causes reduced amplitude
  FX_CHECK(freq > 0.0 || (freq == 0.0 && phase == 0.0));

  // Freqs above num_frames/2 (Nyquist limit) will alias into lower frequencies.
  FX_CHECK(freq * 2.0 <= num_frames) << "Buffer too short--requested frequency will be aliased";

  // freq is defined as: cosine recurs exactly 'freq' times within buf_size.
  const double mult = 2.0 * M_PI / num_frames * freq;

  AudioBuffer<SampleFormat> out(format, num_frames);
  for (size_t frame = 0; frame < num_frames; ++frame) {
    auto val = magn * std::cos(mult * frame + phase);
    switch (SampleFormat) {
      case fuchsia::media::AudioSampleFormat::UNSIGNED_8:
        val = round(val) + 0x80;
        break;
      case fuchsia::media::AudioSampleFormat::SIGNED_16:
      case fuchsia::media::AudioSampleFormat::SIGNED_24_IN_32:
        val = round(val);
        break;
      case fuchsia::media::AudioSampleFormat::FLOAT:
        break;
    }
    for (size_t chan = 0; chan < format.channels(); chan++) {
      out.samples[out.SampleIndex(frame, chan)] = val;
    }
  }
  return out;
}

// Load audio from a WAV file.
template <fuchsia::media::AudioSampleFormat SampleFormat>
AudioBuffer<SampleFormat> LoadWavFile(const std::string& file_name) {
  auto r = WavReader::Open(file_name).take_value();
  auto format = Format::Create({
      .sample_format = r->sample_format(),
      .channels = r->channel_count(),
      .frames_per_second = r->frame_rate(),
  });

  AudioBuffer<SampleFormat> out(format.value(), r->length_in_frames());
  auto size = r->length_in_bytes();
  auto result = r->Read(&out.samples[0], size);
  FX_CHECK(result.is_ok()) << "Read(" << file_name << ") failed, error: " << result.error();
  FX_CHECK(size == result.value())
      << "Read(" << file_name << ") failed, expected " << size << " bytes, got " << result.value();
  return out;
}

struct CompareAudioBufferOptions {
  // See CompareAudioBuffers for a description.
  bool partial = false;

  // If positive, allow the samples to differ from the expected samples by a relative error.
  // If want_slice is empty or RMS(want_slice) == 0, this has no effect.
  // Relative error is defined to be:
  //
  //   abs(RMS(got_slice - want_slice) - RMS(want_slice)) / RMS(want_slice)
  //
  // The term RMS(got_slice - want_slice) is known as RMS error, or RMSE. See:
  // https://en.wikipedia.org/wiki/Root-mean-square_deviation.
  double max_relative_error = 0;

  // These options control additional debugging output of CompareAudioBuffer in failure cases.
  std::string test_label;
  size_t num_frames_per_packet = 100;
};

// Compares got_slice to want_slice, reporting any differences. If got_slice is larger than
// want_slice, the extra suffix should contain silence. If options.partial is true, then got_slice
// should contain a prefix of want_slice, followed by silence.
//
// For example, CompareAudioBuffer succeeds on these inputs
//   got_slice  = {0,1,2,3,4,0,0,0,0,0}
//   want_slice = {0,1,2,3,4}
//   partial    = false
//
// And these inputs:
//   got_slice  = {0,1,2,3,0,0,0,0,0,0}
//   want_slice = {0,1,2,3,4}
//   partial    = true
//
// But not these inputs:
//   got_slice  = {0,1,2,3,0,0,0,0,0,0}
//   want_slice = {0,1,2,3,4}
//   partial    = false
//
// Differences are reported to gtest EXPECT macros.
template <fuchsia::media::AudioSampleFormat SampleFormat>
void CompareAudioBuffers(AudioBufferSlice<SampleFormat> got_slice,
                         AudioBufferSlice<SampleFormat> want_slice,
                         CompareAudioBufferOptions options);

}  // namespace media::audio::test

#endif  // SRC_MEDIA_AUDIO_LIB_TEST_AUDIO_BUFFER_H_
