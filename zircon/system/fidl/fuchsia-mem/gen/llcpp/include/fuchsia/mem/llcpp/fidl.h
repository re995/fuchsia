// WARNING: This file is machine generated by fidlgen.

#pragma once

#include <lib/fidl/internal.h>
#include <lib/fidl/cpp/vector_view.h>
#include <lib/fidl/cpp/string_view.h>
#include <lib/fidl/llcpp/array.h>
#include <lib/fidl/llcpp/coding.h>
#include <lib/fidl/llcpp/traits.h>
#include <lib/fidl/llcpp/transaction.h>
#include <lib/fit/function.h>
#include <lib/zx/vmo.h>
#include <zircon/fidl.h>

namespace fuchsia {
namespace mem {

struct Range;
struct Buffer;
struct Data;

extern "C" const fidl_type_t fuchsia_mem_RangeTable;

// A range of bytes within a VMO.
struct Range {
  static constexpr const fidl_type_t* Type = &fuchsia_mem_RangeTable;
  static constexpr uint32_t MaxNumHandles = 1;
  static constexpr uint32_t PrimarySize = 24;
  [[maybe_unused]]
  static constexpr uint32_t MaxOutOfLine = 0;

  // The vmo that contains the bytes.
  ::zx::vmo vmo{};

  // The offset of the first byte within the range relative to the start of
  // the VMO.
  //
  // For example, if |offset| is zero, then the first byte in the range is
  // the first byte in the VMO.
  uint64_t offset{};

  // The number of bytes in the range.
  //
  // For example, if the offset is 3 and the size is 2, and the VMO starts
  // with "abcdefg...", then the range contains "de".
  //
  // The sum of the offset and the size must not be greater than the
  // physical size of the VMO.
  uint64_t size{};
};

extern "C" const fidl_type_t fuchsia_mem_BufferTable;

// A buffer for data whose size is not necessarily a multiple of the page
// size.
//
// VMO objects have a physical size that is always a multiple of the page
// size. As such, VMO alone cannot serve as a buffer for arbitrarly sized
// data. |fuchsia.mem.Buffer| is a standard struct that aggregate the VMO
// and its size.
struct Buffer {
  static constexpr const fidl_type_t* Type = &fuchsia_mem_BufferTable;
  static constexpr uint32_t MaxNumHandles = 1;
  static constexpr uint32_t PrimarySize = 16;
  [[maybe_unused]]
  static constexpr uint32_t MaxOutOfLine = 0;

  // The vmo that contains the buffer.
  ::zx::vmo vmo{};

  // The number of bytes in the buffer.
  //
  // The content of the buffer begin at the start of the VMO and continue
  // for |size| bytes. To specify a range of bytes that do not start at
  // the beginning of the VMO, use |Range| rather than buffer.
  //
  // This size must not be greater than the physical size of the VMO.
  uint64_t size{};
};

extern "C" const fidl_type_t fuchsia_mem_DataTable;

// Binary data that might be stored inline or in a VMO.
//
// Useful for performance-sensitive protocols that sometimes receive small
// amounts of binary data (i.e., which is more efficient to provide using
// |bytes|) but also need to support arbitrary amounts of data (i.e., which
// need to be provided out-of-line in a |Buffer|).
struct Data {
  Data() : ordinal_(Tag::kUnknown), envelope_{} {}

  enum class Tag : fidl_xunion_tag_t {
    kUnknown = 0,
    kBytes = 835814982,  // 0x31d18646
    kBuffer = 1925873109,  // 0x72ca7dd5
  };

  bool is_bytes() const { return ordinal_ == Tag::kBytes; }

  // The binary data provided inline in the message.
  void set_bytes(::fidl::VectorView<uint8_t>* elem) {
    ordinal_ = Tag::kBytes;
    envelope_.data = static_cast<void*>(elem);
  }

  // The binary data provided inline in the message.
  ::fidl::VectorView<uint8_t>& bytes() const {
    ZX_ASSERT(ordinal_ == Tag::kBytes);
    return *static_cast<::fidl::VectorView<uint8_t>*>(envelope_.data);
  }

  bool is_buffer() const { return ordinal_ == Tag::kBuffer; }

  // The binary data provided out-of-line in a |Buffer|.
  void set_buffer(Buffer* elem) {
    ordinal_ = Tag::kBuffer;
    envelope_.data = static_cast<void*>(elem);
  }

  // The binary data provided out-of-line in a |Buffer|.
  Buffer& buffer() const {
    ZX_ASSERT(ordinal_ == Tag::kBuffer);
    return *static_cast<Buffer*>(envelope_.data);
  }

  Tag which() const;

  static constexpr const fidl_type_t* Type = &fuchsia_mem_DataTable;
  static constexpr uint32_t MaxNumHandles = 1;
  static constexpr uint32_t PrimarySize = 24;
  [[maybe_unused]]
  static constexpr uint32_t MaxOutOfLine = 4294967295;

 private:
  static void SizeAndOffsetAssertionHelper();
  Tag ordinal_;
  FIDL_ALIGNDECL
  fidl_envelope_t envelope_;
};

}  // namespace mem
}  // namespace fuchsia

namespace fidl {

template <>
struct IsFidlType<::fuchsia::mem::Range> : public std::true_type {};
static_assert(std::is_standard_layout_v<::fuchsia::mem::Range>);
static_assert(offsetof(::fuchsia::mem::Range, vmo) == 0);
static_assert(offsetof(::fuchsia::mem::Range, offset) == 8);
static_assert(offsetof(::fuchsia::mem::Range, size) == 16);
static_assert(sizeof(::fuchsia::mem::Range) == ::fuchsia::mem::Range::PrimarySize);

template <>
struct IsFidlType<::fuchsia::mem::Buffer> : public std::true_type {};
static_assert(std::is_standard_layout_v<::fuchsia::mem::Buffer>);
static_assert(offsetof(::fuchsia::mem::Buffer, vmo) == 0);
static_assert(offsetof(::fuchsia::mem::Buffer, size) == 8);
static_assert(sizeof(::fuchsia::mem::Buffer) == ::fuchsia::mem::Buffer::PrimarySize);

template <>
struct IsFidlType<::fuchsia::mem::Data> : public std::true_type {};
static_assert(std::is_standard_layout_v<::fuchsia::mem::Data>);

}  // namespace fidl
