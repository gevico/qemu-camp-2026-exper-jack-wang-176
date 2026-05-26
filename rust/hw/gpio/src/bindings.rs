//消除rust烦人要求
#![allow(
    dead_code,
    improper_ctypes_definitions,
    improper_ctypes,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    unnecessary_transmutes,
    unsafe_op_in_unsafe_fn,
    clippy::pedantic,
    clippy::restriction,
    clippy::style,
    clippy::missing_const_for_fn,
    clippy::ptr_offset_with_cast,
    clippy::useless_transmute,
    clippy::missing_safety_doc,
    clippy::too_many_arguments
)]
//后续添加c对接代码，



// use std::os::raw::{c_void,c_int};
// use hwcore::prelude::DeviceState;

/* 
#[repr(C)]
//这个只是从c层对接的一个占位符，实际使用中会被替换成具体的irq对象
//只是对上层指针贴的标签，在业务代码中我们不会对这个变量进行解引用所以在地址空间上不需要和c语言对齐
//实际的指针内容会在c语言层面进行构建和管理，我们在rust层面只需要保证这个指针的类型正确即可
//这里enum使得用户无法直接创建这个类型的实例，保证了安全性，同时又能满足c语言层面对这个类型的需求
pub enum IRQState{}
//这个函数指针类型是为了适配c语言层面的中断处理函数设计的，c语言层面会传入一个void指针作为上下文参数，以及中断号和中断电平等信息，
//我们在rust层面定义这个类型来匹配c语言的函数指针类型，以便我们能够在rust层面实现对应的中断处理逻辑，并且能够被c语言层面调用
//回调函数的注册实现了c语言的面向对象机制
pub type qemu_irq_handler = Option<unsafe extern "C" fn(opaque: *mut c_void, n: c_int, level: c_int)>;
extern "C"{
    //注册gpio输出中断，传入设备指针和一个指向irq状态指针数组的指针，以及数组长度，这样c语言层面就可以为每个GPIO引脚分配一个独立的中断状态对象，并且我们在rust层面可以通过这个数组来管理和触发
    pub fn qdev_init_gpio_out(dev: *mut DeviceState, pins: *mut *mut IRQState, n: i32);
    //这个函数是为了在rust层面触发中断信号的，传入一个指向irq状态对象的指针和一个表示中断电平的整数，这样我们就可以在rust层面根据业务逻辑来控制中断的触发和电平变化，并且这个函数会被c语言层面调用来实现中断的实际触发
    pub fn qemu_set_irq(irq: *mut IRQState, level: i32);
    //注册gpio输入中断，传入设备指针、一个函数指针作为中断处理函数，以及一个整数表示中断数量，这样c语言层面就可以为每个GPIO输入引脚注册一个独立的中断处理函数，并且我们在rust层面可以通过这个函数来管理和触发输入引脚的中断
    pub fn qdev_init_gpio_in(dev: *mut DeviceState, handler: qemu_irq_handler, n: i32);
}
    */