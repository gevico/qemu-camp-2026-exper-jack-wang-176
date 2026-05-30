use std::cell::UnsafeCell;
use std::ffi::CStr;
use std::pin::Pin;

use common::prelude::*;
use hwcore::prelude::*;
use qom::prelude::*;
use system::prelude::*;
use util::timer::{Timer, CLOCK_VIRTUAL};

pub const TYPE_PWM: &CStr = c"pwm";

qom_isa!(PwmState: SysBusDevice, DeviceState, Object);

unsafe impl ObjectType for PwmState {
    type Class = <SysBusDevice as ObjectType>::Class;
    const TYPE_NAME: &'static CStr = TYPE_PWM;
}

#[repr(C)]
#[derive(qom::Object, hwcore::Device)]
pub struct PwmState {
    parent_obj: ParentField<SysBusDevice>,
    mmio: MemoryRegion,
    
    // 0x00: 全局控制寄存器
    glb: UnsafeCell<u32>,
    // 0x04-0x0c: 4个通道的控制寄存器
    ctrl: UnsafeCell<[u32; 4]>,
    // 0x10-0x1c: 4个通道的周期寄存器
    period: UnsafeCell<[u32; 4]>,
    // 0x20-0x2c: 4个通道的占空比寄存器
    duty: UnsafeCell<[u32; 4]>,

    cycle_start: UnsafeCell<[u64; 4]>, // 每个通道的周期开始时间
    phase: UnsafeCell<[u32; 4]>,       // 当前相位标志：0 前端duty前 1 后端 duty后

    // 4个计时器
    timers: [Timer; 4],

    irq: InterruptSource,
    // 时钟中断对应的外部导线
    outlines: [InterruptSource; 4],
}

unsafe impl Send for PwmState {}
unsafe impl Sync for PwmState {}

impl ObjectImpl for PwmState {
    type ParentType = SysBusDevice;
    const CLASS_INIT: fn(&mut Self::Class) = Self::Class::class_init::<Self>;
    const INSTANCE_INIT: Option<unsafe fn(ParentInit<Self>)> = Some(Self::init);
    const INSTANCE_POST_INIT: Option<fn(&Self)> = Some(Self::post_init);
}

impl DeviceImpl for PwmState {}
impl ResettablePhasesImpl for PwmState {}
impl SysBusDeviceImpl for PwmState {}

impl PwmState {
    unsafe fn init(mut this: ParentInit<Self>) {
        static PWM_OPS: MemoryRegionOps<PwmState> = MemoryRegionOpsBuilder::<PwmState>::new()
            .read(&PwmState::read)
            .write(&PwmState::write)
            .little_endian()
            .impl_sizes(4, 4)
            .build();

        MemoryRegion::init_io(
            &mut uninit_field_mut!(*this, mmio),
            &PWM_OPS,
            "pwm",
            0x1000, // 应当是 0x1000 而不是 0x100
        );

        uninit_field_mut!(*this, glb).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, ctrl).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this, period).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this, duty).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this, cycle_start).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this, phase).write(UnsafeCell::new([0; 4]));
        
        // 修正 Timer 的初始化：它是 unsafe 的，不带参数
        uninit_field_mut!(*this, timers).write([
            unsafe { Timer::new() }, unsafe { Timer::new() },
            unsafe { Timer::new() }, unsafe { Timer::new() }
        ]);
        
        uninit_field_mut!(*this, irq).write(InterruptSource::default());
        uninit_field_mut!(*this, outlines).write([
            InterruptSource::default(), InterruptSource::default(),
            InterruptSource::default(), InterruptSource::default()
        ]);
    }

    #[allow(invalid_reference_casting)]
    fn post_init(&self) {
        self.init_mmio(&self.mmio);
        self.init_irq(&self.irq);
        // 修正：应调用 init_gpio_out
        self.init_gpio_out(&self.outlines);
        
        unsafe {
            Timer::init_full(Pin::new_unchecked(&mut *(self as *const Self as *mut Self)), None, CLOCK_VIRTUAL, Timer::NS, 0, PwmState::time0_handler, |s| &mut s.timers[0]);
            Timer::init_full(Pin::new_unchecked(&mut *(self as *const Self as *mut Self)), None, CLOCK_VIRTUAL, Timer::NS, 0, PwmState::time1_handler, |s| &mut s.timers[1]);
            Timer::init_full(Pin::new_unchecked(&mut *(self as *const Self as *mut Self)), None, CLOCK_VIRTUAL, Timer::NS, 0, PwmState::time2_handler, |s| &mut s.timers[2]);
            Timer::init_full(Pin::new_unchecked(&mut *(self as *const Self as *mut Self)), None, CLOCK_VIRTUAL, Timer::NS, 0, PwmState::time3_handler, |s| &mut s.timers[3]);   
        }
    }
}

// 工具函数
impl PwmState {
    fn time0_handler(&self) { self.handler_timer(0); }
    fn time1_handler(&self) { self.handler_timer(1); }
    fn time2_handler(&self) { self.handler_timer(2); }
    fn time3_handler(&self) { self.handler_timer(3); }

    #[inline(always)]
    fn glb_val(&self) -> u32 {
        unsafe { *self.glb.get() }
    }
    
    #[inline(always)]
    fn ctrl_val(&self, n: usize) -> u32 {
        unsafe { (*self.ctrl.get())[n] } 
    }
    
    #[inline(always)]
    fn duty_val(&self, n: usize) -> u32 {
        unsafe { (*self.duty.get())[n] } 
    }
    
    #[inline(always)]
    fn period_val(&self, n: usize) -> u32 {
        unsafe { (*self.period.get())[n] }
    }
    
    #[inline(always)]
    fn cycle_start_val(&self, n: usize) -> u64 {
        unsafe { (*self.cycle_start.get())[n] } 
    }

    #[inline(always)]
    fn phase_val(&self, n: usize) -> u32 {
        unsafe { (*self.phase.get())[n] } 
    }
    
    #[inline(always)]
    fn glb_set(&self, val: u32) {
        unsafe { *self.glb.get() = val; }
    }
    
    #[inline(always)]
    fn ctrl_set(&self, n: usize, val: u32) {
        unsafe { (*self.ctrl.get())[n] = val; }
    }
    
    #[inline(always)]
    fn duty_set(&self, n: usize, val: u32) {
        unsafe { (*self.duty.get())[n] = val; }
    }
    
    #[inline(always)]
    fn period_set(&self, n: usize, val: u32) {
        unsafe { (*self.period.get())[n] = val; }
    }
    
    #[inline(always)]
    fn cycle_start_set(&self, n: usize, val: u64) {
        unsafe { (*self.cycle_start.get())[n] = val; }
    }
    
    #[inline(always)]
    fn phase_set(&self, n: usize, val: u32) {
        unsafe { (*self.phase.get())[n] = val; }
    }
    
    #[inline(always)]
    fn glb_set_bit(&self) -> u32 {
        let mut val = self.glb_val();
        for i in 0..4 {
            let ctrl = self.ctrl_val(i);
            if (ctrl & 1) != 0 {
                val |= 1 << i;
            }
        }
        val
    }
    
    #[inline(always)]
    fn glb_update_bit(&self, val: u32) {
        let mask = (val & 0xf0) | 0x0f;
        let mut glb = self.glb_val();
        glb &= !mask;
        self.glb_set(glb);
        self.update_irq();
    }
}

// 具体的时钟函数
impl PwmState {
    fn update_irq(&self) {
        let glb = self.glb_val();
        let mut irq_triggered = false;
        for i in 0..4 {
            let done = (glb & (1 << (4 + i))) != 0;
            let enabled = (self.ctrl_val(i) & (1 << 2)) != 0;
            if done && enabled {
                irq_triggered = true;
                break;
            }
        }
        self.irq.set(irq_triggered);
    }
    
    fn start_pwm_timer(&self, n: usize, now: u64) {
        let ctrl = self.ctrl_val(n);
        let period = self.period_val(n);
        let duty = self.duty_val(n);
        let timer = &self.timers[n];
        let pol = (ctrl & (1 << 1)) != 0;
        self.cycle_start_set(n, now);

        if duty == 0 {
            self.outlines[n].set(pol);
            self.phase_set(n, 1);
            timer.modify_ns(now + period as u64); // 修正：modify_ns
        } else if duty >= period {
            self.outlines[n].set(!pol);
            self.phase_set(n, 1);
            timer.modify_ns(now + period as u64);
        } else {
            self.outlines[n].set(!pol);
            self.phase_set(n, 0);
            timer.modify_ns(now + duty as u64);
        }
    }
    
    fn handler_timer(&self, i: usize) {
        let now = CLOCK_VIRTUAL.get_ns(); // 修正：get_ns()
        let phase = self.phase_val(i);
        if phase == 0 {
            let ctrl = self.ctrl_val(i);
            let pol = (ctrl & (1 << 1)) != 0;
            let period = self.period_val(i);
            let start = self.cycle_start_val(i);
            self.outlines[i].set(pol);
            self.phase_set(i, 1);
            self.timers[i].modify_ns(start + period as u64); // 修正
        } else {
            let glb = self.glb_val() | (1 << (i + 4));
            self.glb_set(glb);
            self.update_irq();
            self.start_pwm_timer(i, now);
        }
    }
}

impl PwmState {
    fn read(&self, offset: hwaddr, _size: u32) -> u64 {
        if offset == 0x00 {
            return self.glb_set_bit() as u64;
        } else if offset >= 0x10 && offset < 0x50 {
            let n = ((offset - 0x10) / 0x10) as usize;
            let reg = (offset - 0x10) % 0x10;
            return match reg {
                0x00 => self.ctrl_val(n) as u64,
                0x04 => self.period_val(n) as u64,
                0x08 => self.duty_val(n) as u64,
                0x0C => {
                    if (self.ctrl_val(n) & 1) == 0 {
                        return 0;
                    }
                    let now = CLOCK_VIRTUAL.get_ns();
                    let start = self.cycle_start_val(n);
                    now.saturating_sub(start) as u64
                },
                _ => 0
            };
        }
        0
    }
    
    fn write(&self, offset: hwaddr, data: u64, _size: u32) {
        if offset == 0x00 {
            self.glb_update_bit(data as u32);
        } else if offset >= 0x10 && offset < 0x50 {
            let n = ((offset - 0x10) / 0x10) as usize;
            let reg = (offset - 0x10) % 0x10;
            match reg {
                0x00 => {
                    let old_ctrl = self.ctrl_val(n);
                    self.ctrl_set(n, data as u32);
                    let old_en = (old_ctrl & 1) != 0;
                    let new_en = (data as u32 & 1) != 0;
                    if !old_en && new_en {
                        let now = CLOCK_VIRTUAL.get_ns();
                        self.start_pwm_timer(n, now);
                    } else if old_en && !new_en {
                        self.timers[n].delete();
                        let pol = (data as u32 & (1 << 1)) != 0;
                        self.outlines[n].set(pol);
                    }
                    self.update_irq(); // 修正：当 CTRL 改变时，INTIE 位可能改变，需更新中断
                },
                0x04 => self.period_set(n, data as u32),
                0x08 => self.duty_set(n, data as u32),
                0x0C => {}, // 只读
                _ => {}
            }
        }
    }
}