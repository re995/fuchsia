// Copyright 2021 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

#include "magma_image.h"

#include <fuchsia/sysmem/llcpp/fidl.h>
#include <lib/image-format-llcpp/image-format-llcpp.h>
#include <lib/service/llcpp/service.h>
#include <lib/syslog/global.h>
#include <lib/zx/channel.h>

#include <cassert>

#include <src/lib/fsl/handles/object_info.h>

#include "drm_fourcc.h"

#include <vulkan/vulkan.hpp>

#define LOG_VERBOSE(msg, ...) \
  if (true)                   \
  FX_LOGF(INFO, "magma_image", msg, ##__VA_ARGS__)

namespace {

static uint32_t to_uint32(uint64_t value) {
  assert(value <= std::numeric_limits<uint32_t>::max());
  return static_cast<uint32_t>(value);
}

static uint64_t SysmemModifierToDrmModifier(uint64_t modifier) {
  switch (modifier) {
    case fuchsia_sysmem::wire::FORMAT_MODIFIER_LINEAR:
      return DRM_FORMAT_MOD_LINEAR;
    case fuchsia_sysmem::wire::FORMAT_MODIFIER_INTEL_I915_X_TILED:
      return I915_FORMAT_MOD_X_TILED;
    case fuchsia_sysmem::wire::FORMAT_MODIFIER_INTEL_I915_Y_TILED:
      return I915_FORMAT_MOD_Y_TILED;
    case fuchsia_sysmem::wire::FORMAT_MODIFIER_INTEL_I915_YF_TILED:
      return I915_FORMAT_MOD_Yf_TILED;
  }
  return DRM_FORMAT_MOD_INVALID;
}

class VulkanImageCreator {
 public:
  vk::Result InitVulkan(uint32_t physical_device_index);
  zx_status_t InitSysmem();

  vk::PhysicalDeviceLimits GetPhysicalDeviceLimits();

  // Creates the buffer collection and sets constraints.
  vk::Result CreateCollection(vk::ImageCreateInfo* image_create_info,
                              fuchsia_sysmem::wire::PixelFormatType format,
                              std::vector<uint64_t>& modifiers);

  zx_status_t GetImageInfo(uint32_t width, uint32_t height, zx::vmo* vmo_out,
                           magma_image_info_t* image_info_out);

 private:
  vk::DispatchLoaderDynamic loader_;
  vk::UniqueInstance instance_;
  vk::PhysicalDevice physical_device_;
  vk::UniqueDevice device_;
  fuchsia_sysmem::Allocator::SyncClient sysmem_allocator_;
  fuchsia_sysmem::BufferCollectionToken::SyncClient local_token_;
  fuchsia_sysmem::BufferCollectionToken::SyncClient vulkan_token_;
  fuchsia_sysmem::BufferCollection::SyncClient collection_;
};

vk::Result VulkanImageCreator::InitVulkan(uint32_t physical_device_index) {
  {
    auto app_info =
        vk::ApplicationInfo().setPApplicationName("magma_image").setApiVersion(VK_API_VERSION_1_1);

    auto instance_info = vk::InstanceCreateInfo().setPApplicationInfo(&app_info);

    auto result = vk::createInstanceUnique(instance_info);
    if (result.result != vk::Result::eSuccess) {
      LOG_VERBOSE("Failed to create instance: %s", vk::to_string(result.result).data());
      return result.result;
    }
    instance_ = std::move(result.value);
  }

  assert(instance_);

  {
    auto [result, physical_devices] = instance_->enumeratePhysicalDevices();

    if (result != vk::Result::eSuccess) {
      LOG_VERBOSE("Failed to enumerate physical devices: %s", vk::to_string(result).data());
      return result;
    }
    if (physical_device_index >= physical_devices.size()) {
      LOG_VERBOSE("Invalid physical device index: %d (%zd)", physical_device_index,
                  physical_devices.size());
      return vk::Result::eErrorInitializationFailed;
    }

    physical_device_ = physical_devices[physical_device_index];
  }

  assert(physical_device_);

  {
    const vk::QueueFlags& queue_flags = vk::QueueFlagBits::eGraphics;
    const auto queue_families = physical_device_.getQueueFamilyProperties();

    size_t queue_family_index = queue_families.size();

    for (size_t i = 0; i < queue_families.size(); ++i) {
      if (queue_families[i].queueFlags & queue_flags) {
        queue_family_index = i;
        break;
      }
    }

    if (queue_family_index == queue_families.size()) {
      LOG_VERBOSE("Failed to find queue family with flags 0x%x",
                  static_cast<uint32_t>(queue_flags));
      return vk::Result::eErrorInitializationFailed;
    }

    std::array<const char*, 1> device_extensions{VK_FUCHSIA_BUFFER_COLLECTION_EXTENSION_NAME};

    const float queue_priority = 0.0f;
    auto queue_create_info = vk::DeviceQueueCreateInfo()
                                 .setQueueFamilyIndex(to_uint32(queue_family_index))
                                 .setQueueCount(1)
                                 .setPQueuePriorities(&queue_priority);
    auto device_create_info = vk::DeviceCreateInfo()
                                  .setQueueCreateInfoCount(1)
                                  .setPQueueCreateInfos(&queue_create_info)
                                  .setEnabledExtensionCount(device_extensions.size())
                                  .setPpEnabledExtensionNames(device_extensions.data());

    auto result = physical_device_.createDeviceUnique(device_create_info, nullptr);
    if (result.result != vk::Result::eSuccess) {
      LOG_VERBOSE("Failed to create device: %s", vk::to_string(result.result).data());
      return result.result;
    }

    device_ = std::move(result.value);
  }

  assert(device_);

  loader_.init(instance_.get(), device_.get());

  return vk::Result::eSuccess;
}

zx_status_t VulkanImageCreator::InitSysmem() {
  {
    auto client_end_directory = service::OpenServiceRoot();
    if (!client_end_directory.is_ok()) {
      LOG_VERBOSE("Failed to open service root: %d", client_end_directory.status_value());
      return client_end_directory.status_value();
    }

    auto client_end = service::ConnectAt<fuchsia_sysmem::Allocator>(*client_end_directory);
    if (!client_end.is_ok()) {
      LOG_VERBOSE("Failed to connect to sysmem allocator: %d", client_end.status_value());
      return client_end.status_value();
    }

    sysmem_allocator_ = fuchsia_sysmem::Allocator::SyncClient(std::move(*client_end));
  }

  sysmem_allocator_.SetDebugClientInfo(fidl::unowned_str(fsl::GetCurrentProcessName()),
                                       fsl::GetCurrentProcessKoid());

  {
    auto endpoints = fidl::CreateEndpoints<fuchsia_sysmem::BufferCollectionToken>();
    if (!endpoints.is_ok()) {
      LOG_VERBOSE("Failed to create endpoints: %d", endpoints.status_value());
      return endpoints.status_value();
    }

    auto result = sysmem_allocator_.AllocateSharedCollection(std::move(endpoints->server));
    if (!result.ok()) {
      LOG_VERBOSE("Failed to allocate shared collection: %d", result.status());
      return result.status();
    }

    local_token_ = fuchsia_sysmem::BufferCollectionToken::SyncClient(std::move(endpoints->client));
  }

  {
    auto endpoints = fidl::CreateEndpoints<fuchsia_sysmem::BufferCollectionToken>();
    if (!endpoints.is_ok()) {
      LOG_VERBOSE("Failed to create endpoints: %d", endpoints.status_value());
      return endpoints.status_value();
    }

    constexpr uint32_t kNoRightsAttentuation = ~0;
    auto result = local_token_.Duplicate(kNoRightsAttentuation, std::move(endpoints->server));
    if (!result.ok()) {
      LOG_VERBOSE("Failed to duplicate token: %d", result.status());
      return result.status();
    }

    vulkan_token_ = fuchsia_sysmem::BufferCollectionToken::SyncClient(std::move(endpoints->client));
  }

  {
    auto result = vulkan_token_.Sync();
    if (!result.ok()) {
      LOG_VERBOSE("Failed to sync token: %d", result.status());
      return result.status();
    }
  }

  return ZX_OK;
}

vk::PhysicalDeviceLimits VulkanImageCreator::GetPhysicalDeviceLimits() {
  assert(physical_device_);

  vk::PhysicalDeviceProperties props = physical_device_.getProperties();

  return props.limits;
}

vk::Result VulkanImageCreator::CreateCollection(vk::ImageCreateInfo* image_create_info,
                                                fuchsia_sysmem::wire::PixelFormatType format,
                                                std::vector<uint64_t>& modifiers) {
  assert(device_);

  vk::UniqueHandle<vk::BufferCollectionFUCHSIA, vk::DispatchLoaderDynamic> vk_collection;

  {
    auto collection_create_info = vk::BufferCollectionCreateInfoFUCHSIA().setCollectionToken(
        vulkan_token_.mutable_channel()->release());

    auto result = device_->createBufferCollectionFUCHSIAUnique(collection_create_info,
                                                               nullptr /*pAllocator*/, loader_);
    if (result.result != vk::Result::eSuccess) {
      LOG_VERBOSE("Failed to create buffer collection: %d", result.result);
      return result.result;
    }

    vk_collection = std::move(result.value);
  }

  assert(vk_collection);

  {
    auto image_constraints_info = vk::ImageConstraintsInfoFUCHSIA()
                                      .setCreateInfoCount(1)
                                      .setPCreateInfos(image_create_info)
                                      .setMinBufferCount(1)
                                      .setMaxBufferCount(1);

    auto result = device_->setBufferCollectionImageConstraintsFUCHSIA(
        vk_collection.get(), image_constraints_info, loader_);
    if (result != vk::Result::eSuccess) {
      LOG_VERBOSE("Failed to set constraints: %s", vk::to_string(result).data());
      return result;
    }
  }

  {
    auto endpoints = fidl::CreateEndpoints<fuchsia_sysmem::BufferCollection>();
    if (!endpoints.is_ok()) {
      LOG_VERBOSE("Failed to create endpoints: %d", endpoints.status_value());
      return vk::Result::eErrorInitializationFailed;
    }

    auto result = sysmem_allocator_.BindSharedCollection(std::move(local_token_.client_end()),
                                                         std::move(endpoints->server));
    if (!result.ok()) {
      LOG_VERBOSE("Failed to bind shared collection: %d", result.status());
      return vk::Result::eErrorInitializationFailed;
    }

    collection_ = fuchsia_sysmem::BufferCollection::SyncClient(std::move(endpoints->client));
  }

  {
    fuchsia_sysmem::wire::BufferCollectionConstraints constraints{
        .usage.cpu =
            fuchsia_sysmem::wire::cpuUsageReadOften | fuchsia_sysmem::wire::cpuUsageWriteOften,
        .min_buffer_count = 1,
        .has_buffer_memory_constraints = true,
        .buffer_memory_constraints.cpu_domain_supported = true,
        .image_format_constraints_count = to_uint32(modifiers.size()),
    };

    for (uint32_t index = 0; index < modifiers.size(); index++) {
      fuchsia_sysmem::wire::ImageFormatConstraints& image_constraints =
          constraints.image_format_constraints[index];
      image_constraints = fuchsia_sysmem::wire::ImageFormatConstraints();
      image_constraints.min_coded_width = image_create_info->extent.width;
      image_constraints.min_coded_height = image_create_info->extent.height;
      image_constraints.max_coded_width = image_create_info->extent.width;
      image_constraints.max_coded_height = image_create_info->extent.height;
      image_constraints.min_bytes_per_row = 0;  // Rely on Vulkan to specify
      image_constraints.color_spaces_count = 1;
      image_constraints.color_space[0].type = fuchsia_sysmem::wire::ColorSpaceType::SRGB;
      image_constraints.pixel_format.type = format;
      image_constraints.pixel_format.has_format_modifier = true;
      image_constraints.pixel_format.format_modifier.value = modifiers[index];
    }

    auto result = collection_.SetConstraints(true, constraints);
    if (!result.ok()) {
      LOG_VERBOSE("Failed to set constraints: %d", result.status());
      return vk::Result::eErrorInitializationFailed;
    }
  }

  return vk::Result::eSuccess;
}

zx_status_t VulkanImageCreator::GetImageInfo(uint32_t width, uint32_t height, zx::vmo* vmo_out,
                                             magma_image_info_t* image_info_out) {
  auto result = collection_.WaitForBuffersAllocated();
  if (!result.ok()) {
    LOG_VERBOSE("Failed to wait for buffers allocated: %d", result.status());
    return result.status();
  }

  auto response = result.Unwrap();
  if (response->status != ZX_OK) {
    LOG_VERBOSE("Buffer allocation failed: %d", response->status);
    return response->status;
  }

  fuchsia_sysmem::wire::BufferCollectionInfo_2& collection_info = response->buffer_collection_info;

  if (collection_info.buffer_count != 1) {
    LOG_VERBOSE("Incorrect buffer collection count: %d", collection_info.buffer_count);
    return ZX_ERR_INTERNAL;
  }

  if (!collection_info.buffers[0].vmo.is_valid()) {
    LOG_VERBOSE("Invalid vmo");
    return ZX_ERR_INTERNAL;
  }

  if (collection_info.buffers[0].vmo_usable_start != 0) {
    LOG_VERBOSE("Unsupported vmo usable start: %lu", collection_info.buffers[0].vmo_usable_start);
    return ZX_ERR_INTERNAL;
  }

  std::optional<fuchsia_sysmem::wire::ImageFormat_2> image_format =
      image_format::ConstraintsToFormat(collection_info.settings.image_format_constraints, width,
                                        height);
  if (!image_format) {
    LOG_VERBOSE("Failed to get image format");
    return ZX_ERR_INTERNAL;
  }

  for (uint32_t plane = 0; plane < MAGMA_MAX_IMAGE_PLANES; ++plane) {
    uint64_t offset;
    if (!ImageFormatPlaneByteOffset(*image_format, plane, &offset)) {
      image_info_out->plane_offsets[plane] = 0;
    } else {
      image_info_out->plane_offsets[plane] = to_uint32(offset);
    }

    uint32_t row_bytes;
    if (!image_format::GetPlaneRowBytes(*image_format, plane, &row_bytes)) {
      image_info_out->plane_strides[plane] = 0;
    } else {
      image_info_out->plane_strides[plane] = row_bytes;
    }
  }

  if (image_format->pixel_format.has_format_modifier) {
    image_info_out->drm_format_modifier =
        SysmemModifierToDrmModifier(image_format->pixel_format.format_modifier.value);
  } else {
    image_info_out->drm_format_modifier = DRM_FORMAT_MOD_LINEAR;
  }

  *vmo_out = std::move(collection_info.buffers[0].vmo);

  return ZX_OK;
}

static magma_status_t MagmaStatus(vk::Result result) {
  switch (result) {
    case vk::Result::eSuccess:
      return MAGMA_STATUS_OK;
    case vk::Result::eTimeout:
      return MAGMA_STATUS_TIMED_OUT;
    case vk::Result::eErrorDeviceLost:
      return MAGMA_STATUS_CONNECTION_LOST;
    case vk::Result::eErrorOutOfHostMemory:
    case vk::Result::eErrorOutOfDeviceMemory:
    case vk::Result::eErrorMemoryMapFailed:
      return MAGMA_STATUS_MEMORY_ERROR;
    case vk::Result::eErrorFormatNotSupported:
      return MAGMA_STATUS_INVALID_ARGS;
    default:
      return MAGMA_STATUS_INTERNAL_ERROR;
  }
}

static vk::Format DrmFormatToVulkanFormat(uint32_t drm_format) {
  switch (drm_format) {
    case DRM_FORMAT_ARGB8888:
    case DRM_FORMAT_XRGB8888:
      return vk::Format::eB8G8R8A8Unorm;
    case DRM_FORMAT_ABGR8888:
    case DRM_FORMAT_XBGR8888:
      return vk::Format::eR8G8B8A8Unorm;
  }
  LOG_VERBOSE("Unhandle DRM format: 0x%x", drm_format);
  return vk::Format::eUndefined;
}

static fuchsia_sysmem::wire::PixelFormatType DrmFormatToSysmemFormat(uint32_t drm_format) {
  switch (drm_format) {
    case DRM_FORMAT_ARGB8888:
    case DRM_FORMAT_XRGB8888:
      return fuchsia_sysmem::wire::PixelFormatType::BGRA32;
    case DRM_FORMAT_ABGR8888:
    case DRM_FORMAT_XBGR8888:
      return fuchsia_sysmem::wire::PixelFormatType::R8G8B8A8;
  }
  LOG_VERBOSE("Unhandle DRM format: 0x%x", drm_format);
  return fuchsia_sysmem::wire::PixelFormatType::INVALID;
}

static uint64_t DrmModifierToSysmemModifier(uint64_t modifier) {
  switch (modifier) {
    case DRM_FORMAT_MOD_LINEAR:
      return fuchsia_sysmem::wire::FORMAT_MODIFIER_LINEAR;
    case I915_FORMAT_MOD_X_TILED:
      return fuchsia_sysmem::wire::FORMAT_MODIFIER_INTEL_I915_X_TILED;
    case I915_FORMAT_MOD_Y_TILED:
      return fuchsia_sysmem::wire::FORMAT_MODIFIER_INTEL_I915_Y_TILED;
    case I915_FORMAT_MOD_Yf_TILED:
      return fuchsia_sysmem::wire::FORMAT_MODIFIER_INTEL_I915_YF_TILED;
  }
  LOG_VERBOSE("Unhandle DRM modifier: 0x%x", modifier);
  return fuchsia_sysmem::wire::FORMAT_MODIFIER_INVALID;
}

}  // namespace

namespace magma_image {

magma_status_t CreateDrmImage(uint32_t physical_device_index,
                              const magma_image_create_info_t* create_info,
                              magma_image_info_t* image_info_out, zx::vmo* vmo_out) {
  assert(create_info);
  assert(image_info_out);
  assert(vmo_out);

  if (create_info->flags & ~MAGMA_IMAGE_CREATE_FLAGS_PRESENTABLE) {
    LOG_VERBOSE("Invalid flags: 0x%lx", create_info->flags);
    return MAGMA_STATUS_INVALID_ARGS;
  }

  vk::Format vk_format = DrmFormatToVulkanFormat(create_info->drm_format);
  if (vk_format == vk::Format::eUndefined) {
    LOG_VERBOSE("Invalid format: 0x%lx", create_info->drm_format);
    return MAGMA_STATUS_INVALID_ARGS;
  }

  auto sysmem_format = DrmFormatToSysmemFormat(create_info->drm_format);
  if (sysmem_format == fuchsia_sysmem::wire::PixelFormatType::INVALID) {
    LOG_VERBOSE("Invalid format: 0x%lx", create_info->drm_format);
    return MAGMA_STATUS_INVALID_ARGS;
  }

  std::vector<uint64_t> sysmem_modifiers;

  // Convert modifiers provided by client.
  {
    bool terminator_found = false;
    for (uint32_t i = 0; i < MAGMA_MAX_DRM_FORMAT_MODIFIERS; i++) {
      if (create_info->drm_format_modifiers[i] == DRM_FORMAT_MOD_INVALID) {
        terminator_found = true;
        break;
      }

      uint64_t modifier = DrmModifierToSysmemModifier(create_info->drm_format_modifiers[i]);
      if (modifier == fuchsia_sysmem::wire::FORMAT_MODIFIER_INVALID) {
        LOG_VERBOSE("Invalid modifier: 0x%lx", create_info->drm_format_modifiers[i]);
        return MAGMA_STATUS_INVALID_ARGS;
      }

      sysmem_modifiers.push_back(modifier);
    }

    if (!terminator_found) {
      LOG_VERBOSE("Missing modifier terminator");
      return MAGMA_STATUS_INVALID_ARGS;
    }
  }

  VulkanImageCreator image_creator;

  {
    vk::Result result = image_creator.InitVulkan(physical_device_index);
    if (result != vk::Result::eSuccess) {
      LOG_VERBOSE("Failed to initialize Vulkan");
      return MagmaStatus(result);
    }
  }

  {
    vk::PhysicalDeviceLimits limits = image_creator.GetPhysicalDeviceLimits();
    if (create_info->width > limits.maxImageDimension2D ||
        create_info->height > limits.maxImageDimension2D) {
      LOG_VERBOSE("Invalid width %u or height %u (%u)", create_info->width, create_info->height,
                  limits.maxImageDimension2D);
      return MAGMA_STATUS_INVALID_ARGS;
    }
  }

  zx_status_t status = image_creator.InitSysmem();
  if (status != ZX_OK) {
    LOG_VERBOSE("Failed to initialize sysmem: %d", status);
    return MAGMA_STATUS_INTERNAL_ERROR;
  }

  vk::ImageUsageFlags usage =
      vk::ImageUsageFlagBits::eTransferSrc | vk::ImageUsageFlagBits::eTransferDst |
      vk::ImageUsageFlagBits::eSampled | vk::ImageUsageFlagBits::eStorage |
      vk::ImageUsageFlagBits::eColorAttachment | vk::ImageUsageFlagBits::eInputAttachment;

  auto image_create_info = vk::ImageCreateInfo()
                               .setFormat(vk_format)
                               .setImageType(vk::ImageType::e2D)
                               .setMipLevels(1)
                               .setArrayLayers(1)
                               .setExtent({create_info->width, create_info->height, 1})
                               .setTiling(vk::ImageTiling::eOptimal)
                               .setSharingMode(vk::SharingMode::eExclusive)
                               .setUsage(usage);

  vk::Result result =
      image_creator.CreateCollection(&image_create_info, sysmem_format, sysmem_modifiers);
  if (result != vk::Result::eSuccess) {
    LOG_VERBOSE("Failed to create collection: %s", vk::to_string(result).data());
    return MagmaStatus(result);
  }

  status =
      image_creator.GetImageInfo(create_info->width, create_info->height, vmo_out, image_info_out);
  if (status != ZX_OK) {
    LOG_VERBOSE("GetImageInfo failed: %d", status);
    if (status == ZX_ERR_NOT_SUPPORTED)
      return MAGMA_STATUS_INVALID_ARGS;

    return MAGMA_STATUS_INTERNAL_ERROR;
  }

  return MAGMA_STATUS_OK;
}

}  // namespace magma_image