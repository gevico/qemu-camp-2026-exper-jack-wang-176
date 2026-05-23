// SPDX-License-Identifier: GPL-2.0-or-later

use std::cell::UnsafeCell;

use std::ffi::CStr;
use common::prelude::*;
use hwcore::prelude::*;
use qom::prelude::*;
use system::prelude::*;

use crate::bindings::{ssi_create_bus, ssi_transfer, SSIBus};

pub const TYPE_RSPI: &CStr = c"g233.rspi";

qom_isa!(RspiState: SysBusDevice, DeviceState, Object);

unsafe impl ObjectType for RspiState {
    type Class = <SysBusDevice as ObjectType>::Class;
    const TYPE_NAME: &'static CStr = TYPE_RSPI;
}

#[repr(C)]
#[derive(qom::Object, hwcore::Device)]
pub struct RspiState {
    parent_obj: ParentField<SysBusDevice>,
    mmio: MemoryRegion,

    cr1: UnsafeCell<u32>,
    cs: UnsafeCell<u32>,
    sr: UnsafeCell<u32>,
    dr: UnsafeCell<u32>,

    irq: InterruptSource,
    cs0_line: InterruptSource,
    ssi_bus: UnsafeCell<*mut SSIBus>,
}

unsafe impl Send for RspiState {}
unsafe impl Sync for RspiState {}

impl ObjectImpl for RspiState {
    type ParentType = SysBusDevice;

    const INSTANCE_INIT: Option<unsafe fn(ParentInit<Self>)> = Some(Self::init);
    const INSTANCE_POST_INIT: Option<fn(&Self)> = Some(Self::post_init);
    const CLASS_INIT: fn(&mut Self::Class) = Self::Class::class_init::<Self>;
}

impl DeviceImpl for RspiState {}

impl ResettablePhasesImpl for RspiState {}

impl SysBusDeviceImpl for RspiState {}

impl RspiState {
    unsafe fn init(mut this: ParentInit<Self>) {
        static RSPI_OPS: MemoryRegionOps<RspiState> = MemoryRegionOpsBuilder::<RspiState>::new()
            .read(&RspiState::read)
            .write(&RspiState::write)
            .little_endian()
            .impl_sizes(4, 4)
            .build();

        MemoryRegion::init_io(
            &mut uninit_field_mut!(*this, mmio),
            &RSPI_OPS,
            "rspi",
            0x1000,
        );

        uninit_field_mut!(*this, cr1).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, cs).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, sr).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, dr).write(UnsafeCell::new(0));
    }

    fn post_init(&self) {
        self.init_mmio(&self.mmio);
        self.init_irq(&self.irq);
        self.init_irq(&self.cs0_line);
        self.cs0_line.set(true);

        let bus = unsafe {
            ssi_create_bus(
                self.as_mut_ptr() as *mut DeviceState,
                b"ssi\0".as_ptr() as *const i8,
            )
        };
        unsafe {
            *self.ssi_bus.get() = bus;
        }
    }
}

impl RspiState {
    fn cr1_val(&self) -> u32 {
        unsafe { *self.cr1.get() }
    }

    fn set_cr1(&self, val: u32) {
        unsafe { *self.cr1.get() = val; }
    }

    fn cs_val(&self) -> u32 {
        unsafe { *self.cs.get() }
    }

    fn set_cs(&self, val: u32) {
        unsafe { *self.cs.get() = val; }
    }

    fn sr_val(&self) -> u32 {
        unsafe { *self.sr.get() }
    }

    fn set_sr(&self, val: u32) {
        unsafe { *self.sr.get() = val; }
    }

    fn dr_val(&self) -> u32 {
        unsafe { *self.dr.get() }
    }

    fn set_dr(&self, val: u32) {
        unsafe { *self.dr.get() = val; }
    }

    fn ssi_bus_ptr(&self) -> *mut SSIBus {
        unsafe { *self.ssi_bus.get() }
    }

    fn update_irq(&self) {
        let cr1 = self.cr1_val();
        let sr = self.sr_val();

        let txe_ie = (cr1 & (1 << 7)) != 0;
        let rxne_ie = (cr1 & (1 << 6)) != 0;
        let err_ie = (cr1 & (1 << 5)) != 0;
        let overrun = (sr & (1 << 4)) != 0;
        let txe = (sr & (1 << 1)) != 0;
        let rxne = (sr & (1 << 0)) != 0;

        let trigger_bool = (txe_ie && txe) || (rxne_ie && rxne) || (err_ie && overrun);
        self.irq.set(trigger_bool);
    }

    fn read(&self, offset: hwaddr, _size: u32) -> u64 {
        match offset {
            0x00 => self.cr1_val() as u64,
            0x04 => self.sr_val() as u64,
            0x08 => {
                let data = self.dr_val();
                self.set_sr(self.sr_val() & !(1 << 0));
                self.update_irq();
                data as u64
            }
            0x0C => self.cs_val() as u64,
            _ => 0,
        }
    }

    fn write(&self, offset: hwaddr, value: u64, _size: u32) {
        let val32 = value as u32;
        match offset {
            0x00 => {
                let old_cr1 = self.cr1_val();
                self.set_cr1(val32);
                if (old_cr1 & 1) == 0 && (val32 & 1) != 0 {
                    self.set_sr(self.sr_val() | (1 << 1));
                }
                if (val32 & 1) == 0 {
                    self.set_sr(0);
                }
                self.update_irq();
            }
            0x04 => {
                if (val32 & (1 << 4)) != 0 {
                    self.set_sr(self.sr_val() & !(1 << 4));
                    self.update_irq();
                }
            }
            0x08 => {
                let sr_before = self.sr_val();
                if (sr_before & (1 << 0)) != 0 {
                    self.set_sr(sr_before | (1 << 4));
                }
                self.set_sr(self.sr_val() & !(1 << 1));
                let flash_data = unsafe { ssi_transfer(self.ssi_bus_ptr(), val32) };
                self.set_dr(flash_data);
                self.set_sr(self.sr_val() | (1 << 0));
                self.set_sr(self.sr_val() | (1 << 1));
                self.update_irq();
            }
            0x0C => {
                let old_cs = self.cs_val() & 0x03;
                let new_cs = val32 & 0x03;
                if new_cs != old_cs {
                    if new_cs == 0 {
                        self.cs0_line.set(false);
                    } else {
                        self.cs0_line.set(true);
                    }
                }
                self.set_cs(val32);
                self.update_irq();
            }
            _ => {}
        }
    }
}