use std::cell::UnsafeCell;
use std::ffi::CStr;
use std::pin::Pin;

use common::prelude::*;
use hwcore::prelude::*;
use qom::prelude::*;
use system::prelude::*;
use util::timer::{Timer,CLOCK_VIRTUAL};

pub const TYPE_WDT: &CStr = c"wdt";
pub const WDT_FEED : u32 = 0x5A5A_5A5A;
pub const WDT_LOCK : u32 = 0x1ACC_E551;

qom_isa!(WdtState: SysBusDevice, DeviceState, Object);

unsafe impl ObjectType for WdtState {
    type Class = <SysBusDevice as ObjectType>::Class;
    const TYPE_NAME: &'static CStr = TYPE_WDT;
}


/*Offset	寄存器	访问	复位值	描述
0x00	WDT_CTRL	R/W	0x0000_0000	控制寄存器
0x04	WDT_LOAD	R/W	0x0000_FFFF	装载值寄存器
0x08	WDT_VAL	R	0x0000_FFFF	当前计数值（只读）
0x0C	WDT_SR	R/W	0x0000_0000	状态寄存器
0x10	WDT_KEY	W	—	密钥寄存器（只写） */
#[repr(C)]
#[derive(qom::Object, hwcore::Device)]
pub struct WdtState {
    parent_obj: ParentField<SysBusDevice>,
    mmio: MemoryRegion,

    //0:使能 1:中断使能 2:重置使能 3:锁定寄存器
    ctrl: UnsafeCell<u32>,
    // 0-15:装载值 重置为·0xffff    
    load: UnsafeCell<u32>,
    //倒计时 重置为0xffff 只读
    val:  UnsafeCell<u32>,
    sr:   UnsafeCell<u32>,
    key:  UnsafeCell<u32>,

    timer: Timer,
    irq: InterruptSource,
    feed_timer:  UnsafeCell<u64>,
}
unsafe impl Send for WdtState {}
unsafe impl Sync for WdtState {}

impl ObjectImpl for WdtState {
    type ParentType = SysBusDevice;
    const CLASS_INIT: fn(&mut Self::Class) = Self::Class::class_init::<Self>;
    const INSTANCE_INIT: Option<unsafe fn(ParentInit<Self>)> = Some(Self::init);
    const INSTANCE_POST_INIT: Option<fn(&Self)> = Some(Self::post_init);
}

impl DeviceImpl for WdtState {}
//这里后续可能还需要添加
impl ResettablePhasesImpl for WdtState {}
impl SysBusDeviceImpl for WdtState {}

impl WdtState {
    unsafe fn init(mut this: ParentInit<Self>){
        static WDT_OPS : MemoryRegionOps<WdtState> = MemoryRegionOpsBuilder::<WdtState>::new()
            .read(&WdtState::read)        
            .write(&WdtState::write)
            .little_endian()
            .impl_sizes(4,4)
            .build();
        MemoryRegion::init_io(
            &mut uninit_field_mut!(*this,mmio),
            &WDT_OPS,
            "wdt",
            0x1000,
        );
        uninit_field_mut!(*this,ctrl).write(UnsafeCell::new(0));
        uninit_field_mut!(*this,load).write(UnsafeCell::new(0xffff));
        uninit_field_mut!(*this,val).write(UnsafeCell::new(0xffff));
        uninit_field_mut!(*this,sr).write(UnsafeCell::new(0));
        uninit_field_mut!(*this,key).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, feed_timer).write(UnsafeCell::new(0));

        uninit_field_mut!(*this,timer).write(unsafe { Timer::new()});
        uninit_field_mut!(*this,irq).write(InterruptSource::default());


        //只有在mut阶段才有合法的mut指针
        //因此在init阶段就对timer进行初始化
        let self_pin = unsafe { Pin::new_unchecked(&mut *(this.as_mut_ptr())) };
        Timer::init_full(
            self_pin,
            None,
            CLOCK_VIRTUAL,
            Timer::NS,
            0,
            WdtState::timer_handler,
            |s| &mut s.timer
        );
    }
    fn post_init(&self) {
        self.init_mmio(&self.mmio);
        self.init_irq(&self.irq);
    }
}
impl WdtState{
    #[inline(always)]
    fn ctrl_val(&self) -> u32 {
        unsafe {*self.ctrl.get()}
    }
    #[inline(always)]
    fn sr_val(&self) -> u32 {
        unsafe {*self.sr.get()}
    }
    #[inline(always)]
    fn load_val(&self) -> u32 {
        unsafe {*self.load.get()}
    }
  
    #[inline(always)]
    fn ctrl_set(&self, val: u32) {
        unsafe {*self.ctrl.get() = val;}
    }
    #[inline(always)]    
    fn sr_set(&self, val: u32) {
        let old = self.sr_val() & 1;
        let new = old & !(val & 1);
        unsafe {*self.sr.get() = new;}
    }
    #[inline(always)]   
    fn load_set(&self, val: u32) {
        unsafe {*self.load.get() = val;}  
    }
    #[inline(always)]
    fn feed_timer_set(&self, val: u64) {
        unsafe {*self.feed_timer.get() = val;}
    }
    #[inline(always)]
    fn sr_set_out(&self) {
        unsafe {*self.sr.get() |= 1;}
    }
}
extern "C" {
    fn watchdog_perform_action();
}
impl WdtState{
    fn timer_handler(&self){
        self.sr_set_out();
        let irq_en = self.ctrl_val() & (1 << 1) != 0;
        let reset_en = self.ctrl_val() & (1 << 2) != 0;
        if reset_en{
            unsafe { watchdog_perform_action(); }
        }else if irq_en{
            self.update_irq();
        }
    }

    fn update_irq(&self){
        let en = self.ctrl_val() & (1) != 0;
        let irq_en = self.ctrl_val() & (1 << 1) != 0;
        let sr = self.sr_val();
        if en && irq_en && (sr & 1) != 0 {
            self.irq.set(true);
        }else{
            self.irq.set(false);
        }
    }
    fn start_timer(&self){
        let now = CLOCK_VIRTUAL.get_ns();
        self.feed_timer_set(now);
        self.timer.modify_ns(now + self.load_val() as u64);
    }
}
impl WdtState {
    fn read(&self, offset: hwaddr, _size:u32) -> u64 {
        match offset {
            0x00 => return self.ctrl_val() as u64,
            0x04 => return self.load_val() as u64,
            0x08 => {
                let en = self.ctrl_val() & (1) != 0;
                if en{
                    let now = CLOCK_VIRTUAL.get_ns();
                    let elapsed = now.saturating_sub(unsafe { *self.feed_timer.get() });
                    let load = self.load_val() as u64;
                    if elapsed >= load{
                        return 0;
                    } else {
                        return load - elapsed;
                    }
                }else{
                    return self.load_val() as u64;
                }
            },
            0x0C => return self.sr_val() as u64,
            0x10 => return 0, //key寄存器只写
            _ => 0,
        }
    }
    fn write(&self,offset: hwaddr, data: u64, _size:u32){
        let locked = self.ctrl_val() & (1 << 3) != 0;
        if locked && offset != 0x10 {
            //寄存器被锁定，除非是写入key寄存器，否则忽略写操作
            return;
        }
        match offset {
            0x00 =>{
                let old_ctrl = self.ctrl_val();
                self.ctrl_set(data as u32);
                self.update_irq();
                let old_en = old_ctrl & 1 != 0;
                let new_en = data as u32 & 1 != 0;
                if !old_en && new_en {
                    self.start_timer();
                }else if old_en && !new_en {
                    self.timer.delete();
                    self.sr_set(1);
                    self.update_irq();
                }
            },
            0x04 => self.load_set(data as u32),
            0x0C => {
                self.sr_set(data as u32);
                self.update_irq();
            },
            0x10 => {
                let key = data as u32;
                    if key == WDT_FEED {
                        self.start_timer();
                    }else if key == WDT_LOCK{
                        let ctrl = self.ctrl_val();
                        self.ctrl_set(ctrl | (1 << 3));
                    }else{
                        return ;
                    }
                }
            _ => {},
        }
    }
}