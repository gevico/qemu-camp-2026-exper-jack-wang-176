use std::cell::UnsafeCell;

use std::ffi::CStr;
use common::prelude::*;
use hwcore::prelude::*;
use qom::prelude::*;
use system::prelude::*;

//添加bindings c函数依赖
pub const TYPE_GPIO: &CStr = c"gpio";
//添加对象层级
qom_isa!(GpioState: SysBusDevice, DeviceState, Object);
//实现c语言底层内存对接
unsafe impl ObjectType for GpioState{
    type Class = <SysBusDevice as ObjectType>::Class;
    const TYPE_NAME: &'static CStr = TYPE_GPIO;
}


#[repr(C)]
#[derive(qom::Object,hwcore::Device)]
pub struct GpioState{
    parent_obj: ParentField<SysBusDevice>,
    mmio: MemoryRegion,

    dir: UnsafeCell<u32>,
    out: UnsafeCell<u32>, // 0x04: 输出数据寄存器[cite: 1]
    in_pins: UnsafeCell<u32>, // 物理外部输入 (非直接映射，用于合成 0x08 GPIO_IN)
    ie: UnsafeCell<u32>,
    is: UnsafeCell<u32>,
    trig: UnsafeCell<u32>,
    pol: UnsafeCell<u32>,

    irq: InterruptSource,
    outlines: [InterruptSource; 32],
}
//多线程实现
unsafe impl Send for GpioState {}
unsafe impl Sync for GpioState {}

//c语言回调函数，接受外部导线电平变化
//改成rust实现
impl GpioState{
    fn handle_gpio_in(&self,n: u32,level: u32){
        let b = 1 << n;
        if level != 0{
            unsafe { *self.in_pins.get() |= b; }
        }else{
            unsafe { *self.in_pins.get() &= !b; }
        }
        //todo 添加中断状态判断
    }
}

//实现init函数，将初始化ops和内存分布与实际构建初始化分开
impl GpioState{
    unsafe fn init(mut this: ParentInit<Self>){
        static GPIO_OPS: MemoryRegionOps<GpioState> = MemoryRegionOpsBuilder::<GpioState>::new()
        //todo添加read和write的函数实现
            .read(&GpioState::read)
            .write(&GpioState::write)
            .little_endian()
            .impl_sizes(4,4)
            .build();
        MemoryRegion::init_io(
            &mut uninit_field_mut!(*this,mmio),
            &GPIO_OPS,
            "gpio",
            0x1000,
        );
        uninit_field_mut!(*this, dir).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, out).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, in_pins).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, ie).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, is).write(UnsafeCell::new(0));     
        uninit_field_mut!(*this, trig).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, pol).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, irq).write(Default::default());
        uninit_field_mut!(*this, outlines).write(Default::default());
    }
    fn post_init(&self){
        self.init_mmio(&self.mmio);
        self.init_irq(&self.irq);
        self.init_gpio_out(&self.outlines);
        self.init_gpio_in(32, GpioState::handle_gpio_in);
    }
}
//实现objectimpl强制结构，将对应的初始化函数指针绑定到要求const上
impl ObjectImpl for GpioState{
    type ParentType = SysBusDevice;
    
    const CLASS_INIT: fn(&mut Self::Class) = Self::Class::class_init::<Self>;
    const INSTANCE_INIT: Option<unsafe fn(ParentInit<Self>)> = Some(Self::init);
    const INSTANCE_POST_INIT: Option<fn(&Self)> = Some(Self::post_init);
}
//实现deviceimpl和sysbusdeviceimpl强制结构，保证这个对象能够被系统识别为一个设备，并且能够在总线上进行注册和管理
impl DeviceImpl for GpioState{}
impl ResettablePhasesImpl for GpioState{}
impl SysBusDeviceImpl for GpioState{}
//实现工具函数的使用 unsafecell块变量需要在unsafe块中进行访问，这些函数提供了对gpio寄存器值的读取接口，方便我们在业务逻辑中根据需要来获取当前的寄存器状态，并且这些函数的实现也保证了对寄存器值的安全访问，避免了潜在的数据竞争和不一致问题
impl GpioState{
    fn dir_val(&self) -> u32{
        unsafe{ *self.dir.get() }
    }
    fn out_val(&self) -> u32{
        unsafe{ *self.out.get() }
    }
    fn in_pins_val(&self) -> u32{
        unsafe{ *self.in_pins.get()}
    }

    fn ie_val(&self) -> u32{
        unsafe{ *self.ie.get()}
    }
    fn is_val(&self) -> u32{
        unsafe{ *self.is.get()}
    }
    fn trig_val(&self) -> u32{
        unsafe{ *self.trig.get()}
    }
    fn pol_val(&self) -> u32{
        unsafe{ *self.pol.get()}
    }
    fn get_now_pins(&self) -> u32{
        let dir = self.dir_val();
        (self.out_val() & dir) | (self.in_pins_val()&!dir)
    }
}
//read write函数的实现，这些函数是gpio寄存器访问的核心逻辑，read函数根据访问的偏移地址来返回对应寄存器的值，而write函数则根据偏移地址和写入的数据来更新对应寄存器的值，并且在write函数中我们还需要添加一些额外的逻辑来处理特定寄存器的写入操作，比如当写入dir寄存器时，我们需要根据新的方向设置来更新当前引脚的状态，并且当写入is寄存器时，我们需要根据新的中断状态来触发相应的中断信号，这些函数的实现保证了对gpio寄存器的正确访问和业务逻辑的正确执行
impl GpioState{
    fn read(&self,offset: hwaddr,_size: u32)-> u64{
        match offset{
            0x00 => self.dir_val() as u64,
            0x04 => self.out_val() as u64,
            0x08 => self.get_now_pins() as u64,
            0x0c => self.ie_val() as u64,
            0x10 => self.is_val() as u64,
            0x14 => self.trig_val() as u64,
            0x18 => self.pol_val() as u64,
            _ => {
                // log::warn!("gpio: invalid read offset {:#x}",offset);
                0
            }
        }
    }
    fn write(&self,offset: hwaddr,data: u64,_size: u32){
        let val = data as u32;
        match offset{
            0x00 => {
                unsafe { *self.dir.get() = val; }
            }
            0x04 => {
                unsafe { *self.out.get() = val; }
            }
            0x08 => {
                // log::warn!("gpio: write to read-only register GPIO_IN");
            }
            0x0c => {
                unsafe { *self.ie.get() = val; }
            }
            0x10 => {
                unsafe { *self.is.get() = val; }
                //todo 添加中断触发逻辑
            }
            0x14 => {
                unsafe { *self.trig.get() = val; }
            }
            0x18 => {
                unsafe { *self.pol.get() = val; }
            }
            _ => {
                // log::warn!("gpio: invalid write offset {:#x}",offset);
            }
        }
    }
}