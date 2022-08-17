use std::ffi::c_void;
use std::os::raw::c_char;
use std::sync::Arc;
use std::sync::mpsc::{channel, Receiver};
use std::sync::mpsc::RecvTimeoutError::Timeout;
use std::time::Duration;
use erupt::{DeviceLoader, EntryLoader, InstanceLoader, vk};
use erupt::vk::{ApplicationInfo, ApplicationInfoBuilder, BufferCopyBuilder, BufferCreateFlags, BufferCreateInfoBuilder, BufferUsageFlags, CommandBufferAllocateInfoBuilder, CommandBufferBeginInfoBuilder, CommandBufferUsageFlags, CommandPoolCreateInfoBuilder, DependencyFlags, DeviceCreateInfo, DeviceCreateInfoBuilder, DeviceQueueCreateInfoBuilder, EXT_DEBUG_UTILS_EXTENSION_NAME, Fence, FenceCreateInfoBuilder, InstanceCreateInfoBuilder, KHR_SYNCHRONIZATION_2_EXTENSION_NAME, MemoryAllocateInfoBuilder, PhysicalDeviceFeatures, PhysicalDeviceFeatures2Builder, PhysicalDeviceFeaturesBuilder, PhysicalDeviceTimelineSemaphoreFeaturesBuilder, PipelineStageFlags, QueueFlags, RenderPass, Semaphore, SemaphoreCreateFlags, SemaphoreCreateInfoBuilder, SemaphoreType, SemaphoreTypeCreateInfoBuilder, SubmitInfoBuilder, TimelineSemaphoreSubmitInfoBuilder};
use erupt::vk1_0::FenceCreateFlags;

unsafe extern "system" fn vulkan_debug_callback(severity: vk::DebugUtilsMessageSeverityFlagBitsEXT, message_type: vk::DebugUtilsMessageTypeFlagsEXT, callback_data:*const vk::DebugUtilsMessengerCallbackDataEXT, _user_data: *mut std::ffi::c_void) -> vk::Bool32 {
    let str = std::ffi::CStr::from_ptr((*callback_data).p_message);
    println!("vulkan_debug_callback {:?} {:?} {:?}",severity,message_type,str);
    if severity >= vk::DebugUtilsMessageSeverityFlagBitsEXT::WARNING_EXT {
        panic!();
    }
    false.into()
}

fn semaphore_thread(receiver: Receiver<Semaphore>,device_loader: Arc<DeviceLoader>) {
    let mut active_semaphores = Vec::new();
    loop {
        match receiver.recv_timeout(Duration::ZERO) {
            Err(..) => {},
            Ok(item) => {
                println!("new semaphore {item:?}");
                active_semaphores.push(item)
            }
        }
        unsafe {
            active_semaphores = active_semaphores.drain(0..active_semaphores.len()).filter(|&semaphore| {
                if device_loader.get_semaphore_counter_value(semaphore).unwrap() > 0 {
                    println!("dropping semaphore {:?}",semaphore);
                    device_loader.destroy_semaphore(semaphore,None);
                    false
                }
                else {
                    true
                }
            }).collect();
        }
    }
}

const LAYER_KHRONOS_VALIDATION: *const c_char = erupt::cstr!("VK_LAYER_KHRONOS_validation");


fn main() {
    let entry_loader = EntryLoader::new().unwrap();
    let application_info = ApplicationInfoBuilder::new()
        .api_version(vk::make_api_version(1,3,0,0));
    let create_info = InstanceCreateInfoBuilder::new()
        .enabled_extension_names(&[EXT_DEBUG_UTILS_EXTENSION_NAME])
        .enabled_layer_names(&[LAYER_KHRONOS_VALIDATION])
        .application_info(&application_info);
    let instance_loader =unsafe { InstanceLoader::new(&entry_loader, &create_info).unwrap()};
    let debug_messenger_info = erupt::extensions::ext_debug_utils::DebugUtilsMessengerCreateInfoEXTBuilder::new()
        .flags(vk::DebugUtilsMessengerCreateFlagsEXT::all())
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
        .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
        .pfn_user_callback(Some(vulkan_debug_callback));
    unsafe{ instance_loader.create_debug_utils_messenger_ext(&debug_messenger_info, None).unwrap()};

    let device = unsafe{instance_loader.enumerate_physical_devices(None)}.unwrap()[0];

    let queue_properties = unsafe{instance_loader.get_physical_device_queue_family_properties2(device,None, |_| {})};
    let queue = queue_properties.iter().enumerate().find(|p| p.1.queue_family_properties.queue_flags.contains(QueueFlags::GRAPHICS)).map(|p| p.0).unwrap();

    let queue_create_info = DeviceQueueCreateInfoBuilder::new()
        .queue_priorities(&[1.0])
        .queue_family_index(queue.try_into().unwrap());
    let queues = [queue_create_info];
    let mut timeline_semaphore = PhysicalDeviceTimelineSemaphoreFeaturesBuilder::new()
        .timeline_semaphore(true);
    let mut physical_device_features = PhysicalDeviceFeatures2Builder::new();
    physical_device_features.p_next = &mut timeline_semaphore as *mut _ as *mut c_void;
    let mut create_info = DeviceCreateInfoBuilder::new()
        .queue_create_infos(&queues);
    create_info.p_next = &mut physical_device_features as *mut  _ as *const c_void;
    let device_loader = Arc::new(unsafe{DeviceLoader::new(&instance_loader, device, &create_info).unwrap()});
    let command_pool = CommandPoolCreateInfoBuilder::new();
    let command_pool = unsafe{device_loader.create_command_pool(&command_pool,None)}.unwrap();

    let (sender,receiver) = channel();

    let move_device_loader = device_loader.clone();
    std::thread::spawn(move || {
        semaphore_thread(receiver,move_device_loader);
    });
    let allocate_info = CommandBufferAllocateInfoBuilder::new()
        .command_buffer_count(1)
        .command_pool(command_pool);
    let command_buffer = unsafe{device_loader.allocate_command_buffers(&allocate_info).unwrap()}[0];

    let buffer_size = 50_000_000;
    let allocation_size = 100_000_000;
    let buffer_create_info = BufferCreateInfoBuilder::new()
        .usage(BufferUsageFlags::TRANSFER_SRC | BufferUsageFlags::TRANSFER_DST)
        .size(buffer_size);

    let buffer = unsafe{device_loader.create_buffer(&buffer_create_info, None).unwrap()};
    let memory_allocate_info = MemoryAllocateInfoBuilder::new()
        .allocation_size(buffer_size)
        .memory_type_index(0); //Forgive me, I'm not checking if this is the right type to use or not

    let memory = unsafe{device_loader.allocate_memory(&memory_allocate_info,None).unwrap()};
    unsafe{device_loader.bind_buffer_memory(buffer, memory, 0)}.unwrap();
    unsafe {
        let begin_info = CommandBufferBeginInfoBuilder::new()
            .flags(CommandBufferUsageFlags::SIMULTANEOUS_USE);

        device_loader.begin_command_buffer(command_buffer, &begin_info).unwrap();
        let region = BufferCopyBuilder::new()
            .size(buffer_size - 1)
            .src_offset(0)
            .dst_offset(1);
        let regions = [region];
        // device_loader.cmd_copy_buffer(command_buffer, buffer, buffer, &regions);
        // device_loader.cmd_pipeline_barrier(command_buffer, PipelineStageFlags::all(), PipelineStageFlags::all(), DependencyFlags::empty(), &[], &[], &[]);
        device_loader.end_command_buffer(command_buffer).unwrap();
    }
    let mut fences = Vec::new();
    for i in 0..10 {
        let fence_info = FenceCreateInfoBuilder::new()
            .flags(FenceCreateFlags::SIGNALED);
        fences.push(unsafe{device_loader.create_fence(&fence_info,None).unwrap()});
    }
    let mut current_frame = 0;
    let binary_info = SemaphoreCreateInfoBuilder::new();
    let binary_semaphore = unsafe{device_loader.create_semaphore(&binary_info,None).unwrap()};
    loop {
        unsafe{device_loader.wait_for_fences(&[fences[current_frame]], false, u64::MAX).unwrap()};
        unsafe{device_loader.reset_fences(&[fences[current_frame]]).unwrap()}
        let mut semaphore_info = SemaphoreCreateInfoBuilder::new();
        let semaphore_type = SemaphoreTypeCreateInfoBuilder::new()
            .semaphore_type(SemaphoreType::TIMELINE)
            .initial_value(0);
        semaphore_info.p_next = &semaphore_type as *const _ as *const c_void;
        let semaphore = unsafe{device_loader.create_semaphore(&semaphore_info,None).unwrap()};

        sender.send(semaphore).unwrap();
        let queue = unsafe{device_loader.get_device_queue(0, 0)};
        let buffers = [command_buffer];
        let semaphores = [semaphore];
        let mut submit_info = SubmitInfoBuilder::new()
            .wait_semaphores(&[])
            .command_buffers(&buffers)
            .signal_semaphores(&semaphores);
        let timeline_info = TimelineSemaphoreSubmitInfoBuilder::new()
            .signal_semaphore_values(&[1337]);
        submit_info.p_next = &timeline_info as *const _ as *const c_void;
        unsafe{device_loader.queue_submit(queue, &[submit_info], fences[current_frame])}.unwrap();
        current_frame = (current_frame + 1) % fences.len();
    }


}
