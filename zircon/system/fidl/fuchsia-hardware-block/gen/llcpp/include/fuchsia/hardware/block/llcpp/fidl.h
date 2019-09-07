// WARNING: This file is machine generated by fidlgen.

#pragma once

#include <lib/fidl/internal.h>
#include <lib/fidl/cpp/string_view.h>
#include <lib/fidl/llcpp/array.h>
#include <lib/fidl/llcpp/coding.h>
#include <lib/fidl/llcpp/sync_call.h>
#include <lib/fidl/llcpp/traits.h>
#include <lib/fidl/llcpp/transaction.h>
#include <lib/fidl/llcpp/vector_view.h>
#include <lib/fit/function.h>
#include <lib/zx/channel.h>
#include <lib/zx/fifo.h>
#include <lib/zx/vmo.h>
#include <zircon/fidl.h>

#include <fuchsia/storage/metrics/llcpp/fidl.h>

namespace llcpp {

namespace fuchsia {
namespace hardware {
namespace block {

struct BlockStats;
struct VmoID;
class Ftl;
class Block;
struct BlockInfo;



struct BlockStats {
  static constexpr const fidl_type_t* Type = nullptr;
  static constexpr uint32_t MaxNumHandles = 0;
  static constexpr uint32_t PrimarySize = 480;
  [[maybe_unused]]
  static constexpr uint32_t MaxOutOfLine = 0;

  ::llcpp::fuchsia::storage::metrics::CallStat read = {};

  ::llcpp::fuchsia::storage::metrics::CallStat write = {};

  ::llcpp::fuchsia::storage::metrics::CallStat trim = {};

  ::llcpp::fuchsia::storage::metrics::CallStat flush = {};

  ::llcpp::fuchsia::storage::metrics::CallStat barrier_before = {};

  ::llcpp::fuchsia::storage::metrics::CallStat barrier_after = {};
};



struct VmoID {
  static constexpr const fidl_type_t* Type = nullptr;
  static constexpr uint32_t MaxNumHandles = 0;
  static constexpr uint32_t PrimarySize = 2;
  [[maybe_unused]]
  static constexpr uint32_t MaxOutOfLine = 0;

  uint16_t id = {};
};

constexpr uint16_t VMOID_INVALID = 0u;

extern "C" const fidl_type_t fuchsia_hardware_block_FtlFormatResponseTable;

class Ftl final {
  Ftl() = delete;
 public:

  struct FormatResponse final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    int32_t status;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_FtlFormatResponseTable;
    static constexpr uint32_t MaxNumHandles = 0;
    static constexpr uint32_t PrimarySize = 24;
    static constexpr uint32_t MaxOutOfLine = 0;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kResponse;
  };
  using FormatRequest = ::fidl::AnyZeroArgMessage;


  // Collection of return types of FIDL calls in this interface.
  class ResultOf final {
    ResultOf() = delete;
   private:
    template <typename ResponseType>
    class Format_Impl final : private ::fidl::internal::OwnedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::OwnedSyncCallBase<ResponseType>;
     public:
      Format_Impl(zx::unowned_channel _client_end);
      ~Format_Impl() = default;
      Format_Impl(Format_Impl&& other) = default;
      Format_Impl& operator=(Format_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };

   public:
    using Format = Format_Impl<FormatResponse>;
  };

  // Collection of return types of FIDL calls in this interface,
  // when the caller-allocate flavor or in-place call is used.
  class UnownedResultOf final {
    UnownedResultOf() = delete;
   private:
    template <typename ResponseType>
    class Format_Impl final : private ::fidl::internal::UnownedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::UnownedSyncCallBase<ResponseType>;
     public:
      Format_Impl(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);
      ~Format_Impl() = default;
      Format_Impl(Format_Impl&& other) = default;
      Format_Impl& operator=(Format_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };

   public:
    using Format = Format_Impl<FormatResponse>;
  };

  class SyncClient final {
   public:
    explicit SyncClient(::zx::channel channel) : channel_(std::move(channel)) {}
    ~SyncClient() = default;
    SyncClient(SyncClient&&) = default;
    SyncClient& operator=(SyncClient&&) = default;

    const ::zx::channel& channel() const { return channel_; }

    ::zx::channel* mutable_channel() { return &channel_; }

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    ResultOf::Format Format();

    // Caller provides the backing storage for FIDL message via request and response buffers.
    UnownedResultOf::Format Format(::fidl::BytePart _response_buffer);

   private:
    ::zx::channel channel_;
  };

  // Methods to make a sync FIDL call directly on an unowned channel, avoiding setting up a client.
  class Call final {
    Call() = delete;
   public:

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    static ResultOf::Format Format(zx::unowned_channel _client_end);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    static UnownedResultOf::Format Format(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);

  };

  // Messages are encoded and decoded in-place when these methods are used.
  // Additionally, requests must be already laid-out according to the FIDL wire-format.
  class InPlace final {
    InPlace() = delete;
   public:

    static ::fidl::DecodeResult<FormatResponse> Format(zx::unowned_channel _client_end, ::fidl::BytePart response_buffer);

  };

  // Pure-virtual interface to be implemented by a server.
  class Interface {
   public:
    Interface() = default;
    virtual ~Interface() = default;
    using _Outer = Ftl;
    using _Base = ::fidl::CompleterBase;

    class FormatCompleterBase : public _Base {
     public:
      void Reply(int32_t status);
      void Reply(::fidl::BytePart _buffer, int32_t status);
      void Reply(::fidl::DecodedMessage<FormatResponse> params);

     protected:
      using ::fidl::CompleterBase::CompleterBase;
    };

    using FormatCompleter = ::fidl::Completer<FormatCompleterBase>;

    virtual void Format(FormatCompleter::Sync _completer) = 0;

  };

  // Attempts to dispatch the incoming message to a handler function in the server implementation.
  // If there is no matching handler, it returns false, leaving the message and transaction intact.
  // In all other cases, it consumes the message and returns true.
  // It is possible to chain multiple TryDispatch functions in this manner.
  static bool TryDispatch(Interface* impl, fidl_msg_t* msg, ::fidl::Transaction* txn);

  // Dispatches the incoming message to one of the handlers functions in the interface.
  // If there is no matching handler, it closes all the handles in |msg| and closes the channel with
  // a |ZX_ERR_NOT_SUPPORTED| epitaph, before returning false. The message should then be discarded.
  static bool Dispatch(Interface* impl, fidl_msg_t* msg, ::fidl::Transaction* txn);

  // Same as |Dispatch|, but takes a |void*| instead of |Interface*|. Only used with |fidl::Bind|
  // to reduce template expansion.
  // Do not call this method manually. Use |Dispatch| instead.
  static bool TypeErasedDispatch(void* impl, fidl_msg_t* msg, ::fidl::Transaction* txn) {
    return Dispatch(static_cast<Interface*>(impl), msg, txn);
  }

};

extern "C" const fidl_type_t fuchsia_hardware_block_BlockGetInfoResponseTable;
extern "C" const fidl_type_t fuchsia_hardware_block_BlockGetStatsRequestTable;
extern "C" const fidl_type_t fuchsia_hardware_block_BlockGetStatsResponseTable;
extern "C" const fidl_type_t fuchsia_hardware_block_BlockGetFifoResponseTable;
extern "C" const fidl_type_t fuchsia_hardware_block_BlockAttachVmoRequestTable;
extern "C" const fidl_type_t fuchsia_hardware_block_BlockAttachVmoResponseTable;
extern "C" const fidl_type_t fuchsia_hardware_block_BlockCloseFifoResponseTable;
extern "C" const fidl_type_t fuchsia_hardware_block_BlockRebindDeviceResponseTable;

class Block final {
  Block() = delete;
 public:

  struct GetInfoResponse final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    int32_t status;
    BlockInfo* info;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockGetInfoResponseTable;
    static constexpr uint32_t MaxNumHandles = 0;
    static constexpr uint32_t PrimarySize = 32;
    static constexpr uint32_t MaxOutOfLine = 24;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kResponse;
  };
  using GetInfoRequest = ::fidl::AnyZeroArgMessage;

  struct GetStatsResponse final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    int32_t status;
    BlockStats* stats;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockGetStatsResponseTable;
    static constexpr uint32_t MaxNumHandles = 0;
    static constexpr uint32_t PrimarySize = 32;
    static constexpr uint32_t MaxOutOfLine = 480;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kResponse;
  };
  struct GetStatsRequest final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    bool clear;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockGetStatsRequestTable;
    static constexpr uint32_t MaxNumHandles = 0;
    static constexpr uint32_t PrimarySize = 24;
    static constexpr uint32_t MaxOutOfLine = 0;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kRequest;
    using ResponseType = GetStatsResponse;
  };

  struct GetFifoResponse final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    int32_t status;
    ::zx::fifo fifo;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockGetFifoResponseTable;
    static constexpr uint32_t MaxNumHandles = 1;
    static constexpr uint32_t PrimarySize = 24;
    static constexpr uint32_t MaxOutOfLine = 0;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kResponse;
  };
  using GetFifoRequest = ::fidl::AnyZeroArgMessage;

  struct AttachVmoResponse final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    int32_t status;
    VmoID* vmoid;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockAttachVmoResponseTable;
    static constexpr uint32_t MaxNumHandles = 0;
    static constexpr uint32_t PrimarySize = 32;
    static constexpr uint32_t MaxOutOfLine = 8;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kResponse;
  };
  struct AttachVmoRequest final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    ::zx::vmo vmo;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockAttachVmoRequestTable;
    static constexpr uint32_t MaxNumHandles = 1;
    static constexpr uint32_t PrimarySize = 24;
    static constexpr uint32_t MaxOutOfLine = 0;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kRequest;
    using ResponseType = AttachVmoResponse;
  };

  struct CloseFifoResponse final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    int32_t status;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockCloseFifoResponseTable;
    static constexpr uint32_t MaxNumHandles = 0;
    static constexpr uint32_t PrimarySize = 24;
    static constexpr uint32_t MaxOutOfLine = 0;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kResponse;
  };
  using CloseFifoRequest = ::fidl::AnyZeroArgMessage;

  struct RebindDeviceResponse final {
    FIDL_ALIGNDECL
    fidl_message_header_t _hdr;
    int32_t status;

    static constexpr const fidl_type_t* Type = &fuchsia_hardware_block_BlockRebindDeviceResponseTable;
    static constexpr uint32_t MaxNumHandles = 0;
    static constexpr uint32_t PrimarySize = 24;
    static constexpr uint32_t MaxOutOfLine = 0;
    static constexpr bool HasFlexibleEnvelope = false;
    static constexpr ::fidl::internal::TransactionalMessageKind MessageKind =
        ::fidl::internal::TransactionalMessageKind::kResponse;
  };
  using RebindDeviceRequest = ::fidl::AnyZeroArgMessage;


  // Collection of return types of FIDL calls in this interface.
  class ResultOf final {
    ResultOf() = delete;
   private:
    template <typename ResponseType>
    class GetInfo_Impl final : private ::fidl::internal::OwnedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::OwnedSyncCallBase<ResponseType>;
     public:
      GetInfo_Impl(zx::unowned_channel _client_end);
      ~GetInfo_Impl() = default;
      GetInfo_Impl(GetInfo_Impl&& other) = default;
      GetInfo_Impl& operator=(GetInfo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class GetStats_Impl final : private ::fidl::internal::OwnedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::OwnedSyncCallBase<ResponseType>;
     public:
      GetStats_Impl(zx::unowned_channel _client_end, bool clear);
      ~GetStats_Impl() = default;
      GetStats_Impl(GetStats_Impl&& other) = default;
      GetStats_Impl& operator=(GetStats_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class GetFifo_Impl final : private ::fidl::internal::OwnedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::OwnedSyncCallBase<ResponseType>;
     public:
      GetFifo_Impl(zx::unowned_channel _client_end);
      ~GetFifo_Impl() = default;
      GetFifo_Impl(GetFifo_Impl&& other) = default;
      GetFifo_Impl& operator=(GetFifo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class AttachVmo_Impl final : private ::fidl::internal::OwnedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::OwnedSyncCallBase<ResponseType>;
     public:
      AttachVmo_Impl(zx::unowned_channel _client_end, ::zx::vmo vmo);
      ~AttachVmo_Impl() = default;
      AttachVmo_Impl(AttachVmo_Impl&& other) = default;
      AttachVmo_Impl& operator=(AttachVmo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class CloseFifo_Impl final : private ::fidl::internal::OwnedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::OwnedSyncCallBase<ResponseType>;
     public:
      CloseFifo_Impl(zx::unowned_channel _client_end);
      ~CloseFifo_Impl() = default;
      CloseFifo_Impl(CloseFifo_Impl&& other) = default;
      CloseFifo_Impl& operator=(CloseFifo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class RebindDevice_Impl final : private ::fidl::internal::OwnedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::OwnedSyncCallBase<ResponseType>;
     public:
      RebindDevice_Impl(zx::unowned_channel _client_end);
      ~RebindDevice_Impl() = default;
      RebindDevice_Impl(RebindDevice_Impl&& other) = default;
      RebindDevice_Impl& operator=(RebindDevice_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };

   public:
    using GetInfo = GetInfo_Impl<GetInfoResponse>;
    using GetStats = GetStats_Impl<GetStatsResponse>;
    using GetFifo = GetFifo_Impl<GetFifoResponse>;
    using AttachVmo = AttachVmo_Impl<AttachVmoResponse>;
    using CloseFifo = CloseFifo_Impl<CloseFifoResponse>;
    using RebindDevice = RebindDevice_Impl<RebindDeviceResponse>;
  };

  // Collection of return types of FIDL calls in this interface,
  // when the caller-allocate flavor or in-place call is used.
  class UnownedResultOf final {
    UnownedResultOf() = delete;
   private:
    template <typename ResponseType>
    class GetInfo_Impl final : private ::fidl::internal::UnownedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::UnownedSyncCallBase<ResponseType>;
     public:
      GetInfo_Impl(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);
      ~GetInfo_Impl() = default;
      GetInfo_Impl(GetInfo_Impl&& other) = default;
      GetInfo_Impl& operator=(GetInfo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class GetStats_Impl final : private ::fidl::internal::UnownedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::UnownedSyncCallBase<ResponseType>;
     public:
      GetStats_Impl(zx::unowned_channel _client_end, ::fidl::BytePart _request_buffer, bool clear, ::fidl::BytePart _response_buffer);
      ~GetStats_Impl() = default;
      GetStats_Impl(GetStats_Impl&& other) = default;
      GetStats_Impl& operator=(GetStats_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class GetFifo_Impl final : private ::fidl::internal::UnownedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::UnownedSyncCallBase<ResponseType>;
     public:
      GetFifo_Impl(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);
      ~GetFifo_Impl() = default;
      GetFifo_Impl(GetFifo_Impl&& other) = default;
      GetFifo_Impl& operator=(GetFifo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class AttachVmo_Impl final : private ::fidl::internal::UnownedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::UnownedSyncCallBase<ResponseType>;
     public:
      AttachVmo_Impl(zx::unowned_channel _client_end, ::fidl::BytePart _request_buffer, ::zx::vmo vmo, ::fidl::BytePart _response_buffer);
      ~AttachVmo_Impl() = default;
      AttachVmo_Impl(AttachVmo_Impl&& other) = default;
      AttachVmo_Impl& operator=(AttachVmo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class CloseFifo_Impl final : private ::fidl::internal::UnownedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::UnownedSyncCallBase<ResponseType>;
     public:
      CloseFifo_Impl(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);
      ~CloseFifo_Impl() = default;
      CloseFifo_Impl(CloseFifo_Impl&& other) = default;
      CloseFifo_Impl& operator=(CloseFifo_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };
    template <typename ResponseType>
    class RebindDevice_Impl final : private ::fidl::internal::UnownedSyncCallBase<ResponseType> {
      using Super = ::fidl::internal::UnownedSyncCallBase<ResponseType>;
     public:
      RebindDevice_Impl(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);
      ~RebindDevice_Impl() = default;
      RebindDevice_Impl(RebindDevice_Impl&& other) = default;
      RebindDevice_Impl& operator=(RebindDevice_Impl&& other) = default;
      using Super::status;
      using Super::error;
      using Super::ok;
      using Super::Unwrap;
      using Super::value;
      using Super::operator->;
      using Super::operator*;
    };

   public:
    using GetInfo = GetInfo_Impl<GetInfoResponse>;
    using GetStats = GetStats_Impl<GetStatsResponse>;
    using GetFifo = GetFifo_Impl<GetFifoResponse>;
    using AttachVmo = AttachVmo_Impl<AttachVmoResponse>;
    using CloseFifo = CloseFifo_Impl<CloseFifoResponse>;
    using RebindDevice = RebindDevice_Impl<RebindDeviceResponse>;
  };

  class SyncClient final {
   public:
    explicit SyncClient(::zx::channel channel) : channel_(std::move(channel)) {}
    ~SyncClient() = default;
    SyncClient(SyncClient&&) = default;
    SyncClient& operator=(SyncClient&&) = default;

    const ::zx::channel& channel() const { return channel_; }

    ::zx::channel* mutable_channel() { return &channel_; }

    // Allocates 72 bytes of message buffer on the stack. No heap allocation necessary.
    ResultOf::GetInfo GetInfo();

    // Caller provides the backing storage for FIDL message via request and response buffers.
    UnownedResultOf::GetInfo GetInfo(::fidl::BytePart _response_buffer);

    // Allocates 24 bytes of request buffer on the stack. Response is heap-allocated.
    ResultOf::GetStats GetStats(bool clear);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    UnownedResultOf::GetStats GetStats(::fidl::BytePart _request_buffer, bool clear, ::fidl::BytePart _response_buffer);

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    ResultOf::GetFifo GetFifo();

    // Caller provides the backing storage for FIDL message via request and response buffers.
    UnownedResultOf::GetFifo GetFifo(::fidl::BytePart _response_buffer);

    // Allocates 64 bytes of message buffer on the stack. No heap allocation necessary.
    ResultOf::AttachVmo AttachVmo(::zx::vmo vmo);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    UnownedResultOf::AttachVmo AttachVmo(::fidl::BytePart _request_buffer, ::zx::vmo vmo, ::fidl::BytePart _response_buffer);

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    ResultOf::CloseFifo CloseFifo();

    // Caller provides the backing storage for FIDL message via request and response buffers.
    UnownedResultOf::CloseFifo CloseFifo(::fidl::BytePart _response_buffer);

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    ResultOf::RebindDevice RebindDevice();

    // Caller provides the backing storage for FIDL message via request and response buffers.
    UnownedResultOf::RebindDevice RebindDevice(::fidl::BytePart _response_buffer);

   private:
    ::zx::channel channel_;
  };

  // Methods to make a sync FIDL call directly on an unowned channel, avoiding setting up a client.
  class Call final {
    Call() = delete;
   public:

    // Allocates 72 bytes of message buffer on the stack. No heap allocation necessary.
    static ResultOf::GetInfo GetInfo(zx::unowned_channel _client_end);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    static UnownedResultOf::GetInfo GetInfo(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);

    // Allocates 24 bytes of request buffer on the stack. Response is heap-allocated.
    static ResultOf::GetStats GetStats(zx::unowned_channel _client_end, bool clear);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    static UnownedResultOf::GetStats GetStats(zx::unowned_channel _client_end, ::fidl::BytePart _request_buffer, bool clear, ::fidl::BytePart _response_buffer);

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    static ResultOf::GetFifo GetFifo(zx::unowned_channel _client_end);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    static UnownedResultOf::GetFifo GetFifo(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);

    // Allocates 64 bytes of message buffer on the stack. No heap allocation necessary.
    static ResultOf::AttachVmo AttachVmo(zx::unowned_channel _client_end, ::zx::vmo vmo);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    static UnownedResultOf::AttachVmo AttachVmo(zx::unowned_channel _client_end, ::fidl::BytePart _request_buffer, ::zx::vmo vmo, ::fidl::BytePart _response_buffer);

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    static ResultOf::CloseFifo CloseFifo(zx::unowned_channel _client_end);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    static UnownedResultOf::CloseFifo CloseFifo(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);

    // Allocates 40 bytes of message buffer on the stack. No heap allocation necessary.
    static ResultOf::RebindDevice RebindDevice(zx::unowned_channel _client_end);

    // Caller provides the backing storage for FIDL message via request and response buffers.
    static UnownedResultOf::RebindDevice RebindDevice(zx::unowned_channel _client_end, ::fidl::BytePart _response_buffer);

  };

  // Messages are encoded and decoded in-place when these methods are used.
  // Additionally, requests must be already laid-out according to the FIDL wire-format.
  class InPlace final {
    InPlace() = delete;
   public:

    static ::fidl::DecodeResult<GetInfoResponse> GetInfo(zx::unowned_channel _client_end, ::fidl::BytePart response_buffer);

    static ::fidl::DecodeResult<GetStatsResponse> GetStats(zx::unowned_channel _client_end, ::fidl::DecodedMessage<GetStatsRequest> params, ::fidl::BytePart response_buffer);

    static ::fidl::DecodeResult<GetFifoResponse> GetFifo(zx::unowned_channel _client_end, ::fidl::BytePart response_buffer);

    static ::fidl::DecodeResult<AttachVmoResponse> AttachVmo(zx::unowned_channel _client_end, ::fidl::DecodedMessage<AttachVmoRequest> params, ::fidl::BytePart response_buffer);

    static ::fidl::DecodeResult<CloseFifoResponse> CloseFifo(zx::unowned_channel _client_end, ::fidl::BytePart response_buffer);

    static ::fidl::DecodeResult<RebindDeviceResponse> RebindDevice(zx::unowned_channel _client_end, ::fidl::BytePart response_buffer);

  };

  // Pure-virtual interface to be implemented by a server.
  class Interface {
   public:
    Interface() = default;
    virtual ~Interface() = default;
    using _Outer = Block;
    using _Base = ::fidl::CompleterBase;

    class GetInfoCompleterBase : public _Base {
     public:
      void Reply(int32_t status, BlockInfo* info);
      void Reply(::fidl::BytePart _buffer, int32_t status, BlockInfo* info);
      void Reply(::fidl::DecodedMessage<GetInfoResponse> params);

     protected:
      using ::fidl::CompleterBase::CompleterBase;
    };

    using GetInfoCompleter = ::fidl::Completer<GetInfoCompleterBase>;

    virtual void GetInfo(GetInfoCompleter::Sync _completer) = 0;

    class GetStatsCompleterBase : public _Base {
     public:
      void Reply(int32_t status, BlockStats* stats);
      void Reply(::fidl::BytePart _buffer, int32_t status, BlockStats* stats);
      void Reply(::fidl::DecodedMessage<GetStatsResponse> params);

     protected:
      using ::fidl::CompleterBase::CompleterBase;
    };

    using GetStatsCompleter = ::fidl::Completer<GetStatsCompleterBase>;

    virtual void GetStats(bool clear, GetStatsCompleter::Sync _completer) = 0;

    class GetFifoCompleterBase : public _Base {
     public:
      void Reply(int32_t status, ::zx::fifo fifo);
      void Reply(::fidl::BytePart _buffer, int32_t status, ::zx::fifo fifo);
      void Reply(::fidl::DecodedMessage<GetFifoResponse> params);

     protected:
      using ::fidl::CompleterBase::CompleterBase;
    };

    using GetFifoCompleter = ::fidl::Completer<GetFifoCompleterBase>;

    virtual void GetFifo(GetFifoCompleter::Sync _completer) = 0;

    class AttachVmoCompleterBase : public _Base {
     public:
      void Reply(int32_t status, VmoID* vmoid);
      void Reply(::fidl::BytePart _buffer, int32_t status, VmoID* vmoid);
      void Reply(::fidl::DecodedMessage<AttachVmoResponse> params);

     protected:
      using ::fidl::CompleterBase::CompleterBase;
    };

    using AttachVmoCompleter = ::fidl::Completer<AttachVmoCompleterBase>;

    virtual void AttachVmo(::zx::vmo vmo, AttachVmoCompleter::Sync _completer) = 0;

    class CloseFifoCompleterBase : public _Base {
     public:
      void Reply(int32_t status);
      void Reply(::fidl::BytePart _buffer, int32_t status);
      void Reply(::fidl::DecodedMessage<CloseFifoResponse> params);

     protected:
      using ::fidl::CompleterBase::CompleterBase;
    };

    using CloseFifoCompleter = ::fidl::Completer<CloseFifoCompleterBase>;

    virtual void CloseFifo(CloseFifoCompleter::Sync _completer) = 0;

    class RebindDeviceCompleterBase : public _Base {
     public:
      void Reply(int32_t status);
      void Reply(::fidl::BytePart _buffer, int32_t status);
      void Reply(::fidl::DecodedMessage<RebindDeviceResponse> params);

     protected:
      using ::fidl::CompleterBase::CompleterBase;
    };

    using RebindDeviceCompleter = ::fidl::Completer<RebindDeviceCompleterBase>;

    virtual void RebindDevice(RebindDeviceCompleter::Sync _completer) = 0;

  };

  // Attempts to dispatch the incoming message to a handler function in the server implementation.
  // If there is no matching handler, it returns false, leaving the message and transaction intact.
  // In all other cases, it consumes the message and returns true.
  // It is possible to chain multiple TryDispatch functions in this manner.
  static bool TryDispatch(Interface* impl, fidl_msg_t* msg, ::fidl::Transaction* txn);

  // Dispatches the incoming message to one of the handlers functions in the interface.
  // If there is no matching handler, it closes all the handles in |msg| and closes the channel with
  // a |ZX_ERR_NOT_SUPPORTED| epitaph, before returning false. The message should then be discarded.
  static bool Dispatch(Interface* impl, fidl_msg_t* msg, ::fidl::Transaction* txn);

  // Same as |Dispatch|, but takes a |void*| instead of |Interface*|. Only used with |fidl::Bind|
  // to reduce template expansion.
  // Do not call this method manually. Use |Dispatch| instead.
  static bool TypeErasedDispatch(void* impl, fidl_msg_t* msg, ::fidl::Transaction* txn) {
    return Dispatch(static_cast<Interface*>(impl), msg, txn);
  }

};

constexpr uint32_t MAX_TRANSFER_UNBOUNDED = 4294967295u;

constexpr uint32_t FLAG_REMOVABLE = 2u;

constexpr uint32_t FLAG_READONLY = 1u;

constexpr uint32_t FLAG_BOOTPART = 4u;



struct BlockInfo {
  static constexpr const fidl_type_t* Type = nullptr;
  static constexpr uint32_t MaxNumHandles = 0;
  static constexpr uint32_t PrimarySize = 24;
  [[maybe_unused]]
  static constexpr uint32_t MaxOutOfLine = 0;

  uint64_t block_count = {};

  uint32_t block_size = {};

  uint32_t max_transfer_size = {};

  uint32_t flags = {};

  uint32_t reserved = {};
};

}  // namespace block
}  // namespace hardware
}  // namespace fuchsia
}  // namespace llcpp

namespace fidl {

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::BlockStats> : public std::true_type {};
static_assert(std::is_standard_layout_v<::llcpp::fuchsia::hardware::block::BlockStats>);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockStats, read) == 0);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockStats, write) == 80);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockStats, trim) == 160);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockStats, flush) == 240);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockStats, barrier_before) == 320);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockStats, barrier_after) == 400);
static_assert(sizeof(::llcpp::fuchsia::hardware::block::BlockStats) == ::llcpp::fuchsia::hardware::block::BlockStats::PrimarySize);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::VmoID> : public std::true_type {};
static_assert(std::is_standard_layout_v<::llcpp::fuchsia::hardware::block::VmoID>);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::VmoID, id) == 0);
static_assert(sizeof(::llcpp::fuchsia::hardware::block::VmoID) == ::llcpp::fuchsia::hardware::block::VmoID::PrimarySize);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Ftl::FormatResponse> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Ftl::FormatResponse> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Ftl::FormatResponse)
    == ::llcpp::fuchsia::hardware::block::Ftl::FormatResponse::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Ftl::FormatResponse, status) == 16);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::GetInfoResponse> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::GetInfoResponse> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::GetInfoResponse)
    == ::llcpp::fuchsia::hardware::block::Block::GetInfoResponse::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::GetInfoResponse, status) == 16);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::GetInfoResponse, info) == 24);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::GetStatsRequest> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::GetStatsRequest> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::GetStatsRequest)
    == ::llcpp::fuchsia::hardware::block::Block::GetStatsRequest::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::GetStatsRequest, clear) == 16);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::GetStatsResponse> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::GetStatsResponse> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::GetStatsResponse)
    == ::llcpp::fuchsia::hardware::block::Block::GetStatsResponse::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::GetStatsResponse, status) == 16);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::GetStatsResponse, stats) == 24);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::GetFifoResponse> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::GetFifoResponse> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::GetFifoResponse)
    == ::llcpp::fuchsia::hardware::block::Block::GetFifoResponse::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::GetFifoResponse, status) == 16);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::GetFifoResponse, fifo) == 20);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::AttachVmoRequest> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::AttachVmoRequest> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::AttachVmoRequest)
    == ::llcpp::fuchsia::hardware::block::Block::AttachVmoRequest::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::AttachVmoRequest, vmo) == 16);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::AttachVmoResponse> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::AttachVmoResponse> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::AttachVmoResponse)
    == ::llcpp::fuchsia::hardware::block::Block::AttachVmoResponse::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::AttachVmoResponse, status) == 16);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::AttachVmoResponse, vmoid) == 24);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::CloseFifoResponse> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::CloseFifoResponse> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::CloseFifoResponse)
    == ::llcpp::fuchsia::hardware::block::Block::CloseFifoResponse::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::CloseFifoResponse, status) == 16);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::Block::RebindDeviceResponse> : public std::true_type {};
template <>
struct IsFidlMessage<::llcpp::fuchsia::hardware::block::Block::RebindDeviceResponse> : public std::true_type {};
static_assert(sizeof(::llcpp::fuchsia::hardware::block::Block::RebindDeviceResponse)
    == ::llcpp::fuchsia::hardware::block::Block::RebindDeviceResponse::PrimarySize);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::Block::RebindDeviceResponse, status) == 16);

template <>
struct IsFidlType<::llcpp::fuchsia::hardware::block::BlockInfo> : public std::true_type {};
static_assert(std::is_standard_layout_v<::llcpp::fuchsia::hardware::block::BlockInfo>);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockInfo, block_count) == 0);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockInfo, block_size) == 8);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockInfo, max_transfer_size) == 12);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockInfo, flags) == 16);
static_assert(offsetof(::llcpp::fuchsia::hardware::block::BlockInfo, reserved) == 20);
static_assert(sizeof(::llcpp::fuchsia::hardware::block::BlockInfo) == ::llcpp::fuchsia::hardware::block::BlockInfo::PrimarySize);

}  // namespace fidl
