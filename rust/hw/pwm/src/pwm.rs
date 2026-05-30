use std::cell::UnsafeCell;
use std::ffi::CStr;
use std::pin::Pin;

use common::prelude::*;
use hwcore::prelude::*;
use qom::prelude::*;
use system::prelude::*,
use util::timer::{Timer,CLOCK_VIRTUAL};

pub const TYPE_PWM: &CStr = c"pwm";
qom_isa!(PwmState: SysBusDevice, DeviceState, Object);

unsafe impl ObjectType for PwmState{
    type Class <SysBusDevice as ObjectType>::Class;
    const TYPE_NAME: & 'static CStr = TYPE_PWM;
}

#[repr(C)]
#[derive(qom::Object,hwcore::Device)]
pub struct PwmState{
    parent_obj: ParentField<SysBusDevice>,
    mmio: MemoryRegion,
    
    //四个通道
    //0-3 运行状态 4-7 完成标志
    glb: UnsafeCell<u32>, // 0x00: 全局控制寄存器
    //0 通道使能 1 中断极性 2 周期完成中断使能
    ctrl: UnsafeCell<u32,4>, // 0x04-0x0c: 4个通道的控制寄存器
    //归0重载的界限
    peroid: UnsafeCell<u32,4>, // 0x10-0x1c: 4个通道的周期寄存器
    //占比空值 小于输出高电平
    duty: UnsafeCell<u32,4>,
    //在之类current计数器没有实例化，我们在实际进行读取的时候现计算

    cycle_start: UnsafeCell<u64,4>//每个通道的周期开始时间
    //0 前端duty前 1后段 duty后
    phase: UnsafeCell<u32,4>//当前相位标志，记录是被什么唤醒

    //4个计时器
    timers: [Timer;4],

    irq: InterruptSource,
    //时钟中断对应的外部导线
    outlines: [InterruptSource; 4],
}

unsafe impl Send for PwmState {}
unsafe impl Sync for PwmState {}

impl PwmState{
    unsafe fn init(mut this: ParentInit<Self>){
        static PWM_OPS: MemoryRegionOps<PwmState> = MemoryRegionOpsBuilder::<PwmState>::new()
            .read()
            .write()
            .little_endian(),
            .impl_sizes(4,4),
            .build();
        MemoryRegion::init_io(
            &mut uninit_field_mut!(*this,mmio),
            &PWM_OPS,
            "pwm",
            0x100,
        );
        uninit_field_mut!(*this,glb).write(UnsafeCell::new(0));
        uninit_field_mut!(*this,ctrl).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this,peroid).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this,duty).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this,cycle_start).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this,phase).write(UnsafeCell::new([0; 4]));
        uninit_field_mut!(*this,timers).write([Timer::new(CLOCK_VIRTUAL); 4]);
        uninit_field_mut!(*this,irq).write(InterruptSource::default());
        uninit_field_mut!(*this,outlines).write([InterruptSource::default(); 4]);
    }
    fn post_init(&self){
        self.init_mmio(&self.mmio);
        self.init_irq(&self.irq);
        self_gpio_out(&self.outlines);
        unsafe{
            let pin = Pin::new_unchecked(self as *const Self as *mut Self);
            Timer::init_full(pin,NONE,CLOCK_VIRTUAL,Timer::NS,0,self::time0_handler,|s| &mut s.timers[0]);
            Timer::init_full(pin,NONE,CLOCK_VIRTUAL,Timer::NS,0,self::time1_handler,|s| &mut s.timers[1]);
            Timer::init_full(pin,NONE,CLOCK_VIRTUAL,Timer::NS,0,self::time2_handler,|s| &mut s.timers[2]);
            Timer::init_full(pin,NONE,CLOCK_VIRTUAL,Timer::NS,0,self::time3_handler,|s| &mut s.timers[3]);   
        }
    }
}
//工具技能函数
impl PwmState{
    fn time0_handler(&self) {self.time_handler(0);}
    fn time1_handler(&self) {self.time_handler(1);}
    fn time2_handler(&self) {self.time_handler(2);}
    fn time3_handler(&self) {self.time_handler(3);}
    //做为学习示例不适用get_unchecked
    #[inline(always)]
    fn glb_val(&self) -> u32{
        unsafe{(*self.glb.get())[n]}
    }
    /* 
    #[inline(always)]
    fn ctrl_all(&self) -> [u32; 4]{
        unsafe{(*self.ctrl.get())}
    }*/
    #[inline(always)]
    fn ctrl_val(&self, n: usize) -> u32{
        unsafe{(*self.ctrl.get())[n]} 
    }
    #[inline(always)]
    fn duty_val(&self, n: usize) -> u32{
        unsafe{(*self.duty.get())[n]} 
    }
    #[inline(always)]
    fn peroid_val(&self, n: usize) -> u32{
        unsafe{(*self.peroid.get())[n]}
    }
    #[inline(always)]
    fn cycle_start_val(&self, n: usize) -> u64{
        unsafe{(*self.cycle_start.get())[n]} 
    }
    #[inline(always)]
    fn glb_set(&self, val: u32){
        unsafe{(*self.glb.get()) = val;
    }
    #[inline(always)]
    fn ctrl_set(&self, n: usize, val: u32){
        unsafe{(*self.ctrl.get())[n] = val;}
    }
    #[inline(always)]
    fn duty_set(&self, n: usize, val: u32){
        unsafe{(*self.duty.get())[n] = val;}
    }
    #[inline(always)]
    fn peroid_set(&self, n: usize, val: u32){
        unsafe{(*self.peroid.get())[n] = val;
    }
    #[inline(always)]
    fn cycle_start_set(&self, n: usize, val: u64){
        unsafe{(*self.cycle_start.get())[n] = val;
    }
    #[lnline(always)]
    fn phase_set(&self, n: usize, val: u32){
        unsafe{(*self.phase.get())[n] = val;
    }
    #[inline(always)]
    fn glib_set_bit(&self) -> u32{
        let mut val = self.glb_val();
        for i in 0..4{
            let ctrl = self.ctrl_val(i);
            if(ctrl & 1)!= 0{
                val |= 1 <<i
            }
        }
        val;
    }
    #[inline(always)]
    fn glib_update_bit(&self,bit:usize,val:u32){
        let musk = (val & 0xf0) | 0x0f;
        let mut glb = self.glb_val();
        glb &= !musk;
        self.glb_set(glb);
        self.update_irq();
        return;
    }
}
fn ObjectImpl for PwmState{
    type ParentType = SysBusDevice;

    const CLASS_INIT: fn(&mut Self::Class) = Self::Class::class_init::<Self>;
    const INSTANCE_INIT: Option<unsafe fn(ParentInit<Self>)> = Some(Self::init);
    const INSTANCE_POST_INIT: Option<fn(&Self)> = Some(Self::post_init);
}
impl DeviceImpl for PwmState{}
impl ResettablePhasesImpl for PwmState{}
impl SysBusDeviceImpl for PwmState{}
//具体的时钟函数
impl PwmState{
    fn update_irq(&self){
        let glb = self.glb_val();
        let mut irq_trigered = false;
        for i in 0..4{
            let done = (glb & (1 << (4 + i))) != 0;
            let enabled = (self.ctrl_val(i) & (1 << 2)) != 0;
            if done && enabled{
                irq_trigered = true;
                break;
            }
        }
        self.irq.set(irq_trigered);
    }
    fn start_pwm_timer(&self,n:usize,now:u64){
        let ctrl = self.ctrl_val(n);
        let peroid = self.peroid_val(n);
        let duty = self.duty_val(n);
        let timer = &self.timers[n];
        let pol = (ctrl & (1 << 1)) != 0;
        self.cycle_start_set(n,now);

        //phase 0 -> phase 1 有效!pol -> 无效pol
        //phase绑定的唤醒之后的后续操作，和当前的pol无关，pol根据是否处在空占比里面进行设置
        //在占空比阶段输出有效电压
        if duty <= 0{
            self.outlines[n].set(pol);
            self.phase_set(n, 1);
            timer.modify(now + peroid as u64, Timer::NS);
        }else if duty >= peroid{
            self.outlines[n].set(!pol);
            self.phase_set(n,1);
            timer.modify(now + peroid as u64, Timer::NS);
        }else{
            self.outlines[n].set(!pol);
            self.phase_set(n,0);
            timer.modify(now + duty as u64, Timer::NS);
        }
    }
    fn handler_timer(&self,i: usize){
        let now = CLOCK_VIRTUAL.now();
        let phase = self.phase_val(i);
        if phase == 0{
            //处于占空比阶段
            let ctrl = self.ctrl_val(i);
            let pol = (ctrl & (1 << 1)) != 0;
            let peroid = self.peroid_val(i);
            let start = self.cycle_start_val(i);
            self.outlines[i].set(pol);
            self.phase_set(i, 1);
            timer.modify(start + peroid as u64, Timer::NS);
        }else{
            self.glib_set_bit(i);
            self.update_irq();
            self.start_pwm_timer(i, now);
        }
    }
}
impl PwmState{
    fn read(&self,offset: hwaddr,_size: u32)->u64{
        if offset == 0x00{
            self.glib_set_bit() as u64;
        }else if (offset>0x10 && offset<= 0x50) {
            let n = (offset - 0x10) / 0x10;
            let reg = (offset - 0x10)%0x10;
            return match reg{
                0x00 => self.ctrl_val(n as usize) as u64,
                0x04 => self.peroid_val(n as usize) as u64,
                0x08 => {
                    let now = CLOCK_VIRTUAL.now();
                    let start = self.cycle_start_val(n as usize);
                    now.saturating_sub(start) as u64
                },
                _ => 0
            };
        }
        0
    }
    fn write(&self,offset: hwaddr,data: u64,_size: u32){
        if offset = 0x00{
            self.glib_update_bit(0, data as u32);
        }else if(offset > 0x10 && offset <= 0x50){
            let n = (offset - 0x10) / 0x10;
            let reg = (offset - 0x10)%0x10;
            match reg{
                0x00 =>{
                    let old_ctrl = self.ctrl_val(n as usize);
                    self.ctrl_set(n as usize, data as u32);
                    let old_en = (old_ctrl & 1) != 0;
                    let new_en = (data as u32 & 1) != 0;
                    if !old_en && new_en{
                        let now = CLOCK_VIRTUAL.now();
                        self.start_pwm_timer(n as usize,now);
                    }else if old_en && !new_en{
                        self.timers[n as usize].delete();
                        let pol = (data as u 32 & (1 << 1)) != 0;
                        self.outlines[n as usize].set(pol);
                    }
                },
                0x04 => self.peroid_set(n as usize, data as u32),
                0x08 => 0, //周期寄存器不允许写入
                _ => {}
            }
        }
    }
}