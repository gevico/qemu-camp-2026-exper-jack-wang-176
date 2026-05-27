// SPDX-License-Identifier: GPL-2.0-or-later

use std::cell::UnsafeCell;

use std::ffi::CStr;
use common::prelude::*;
use hwcore::prelude::*;
use qom::prelude::*;
use system::prelude::*;

use crate::bindings::{ssi_create_bus, ssi_transfer, SSIBus};

pub const TYPE_SPI: &CStr = c"g233.spi";

qom_isa!(SpiState: SysBusDevice, DeviceState, Object);

unsafe impl ObjectType for SpiState {
    type Class = <SysBusDevice as ObjectType>::Class;
    const TYPE_NAME: &'static CStr = TYPE_SPI;
}

#[repr(C)]
#[derive(qom::Object, hwcore::Device)]
pub struct SpiState {
    parent_obj: ParentField<SysBusDevice>,
    mmio: MemoryRegion,

    cr1: UnsafeCell<u32>,
    cr2: UnsafeCell<u32>,
    sr: UnsafeCell<u32>,
    dr: UnsafeCell<u32>,

    irq: InterruptSource,
    cs0_line: InterruptSource,
    cs1_line: InterruptSource,
    ssi_bus: UnsafeCell<*mut SSIBus>,
}

unsafe impl Send for SpiState {}
unsafe impl Sync for SpiState {}

impl ObjectImpl for SpiState {
    type ParentType = SysBusDevice;

    const INSTANCE_INIT: Option<unsafe fn(ParentInit<Self>)> = Some(Self::init);
    const INSTANCE_POST_INIT: Option<fn(&Self)> = Some(Self::post_init);
    const CLASS_INIT: fn(&mut Self::Class) = Self::Class::class_init::<Self>;
}

impl DeviceImpl for SpiState {}

impl ResettablePhasesImpl for SpiState {
    const HOLD: Option<fn(&Self, ResetType)> = Some(Self::hold_reset);
}

impl SpiState {
    fn hold_reset(&self, _type: ResetType) {
        unsafe { *self.cr1.get() = 0; }
        unsafe { *self.cr2.get() = 0; }
        unsafe { *self.sr.get() = 0x0000_0002; } // Default SR
        unsafe { *self.dr.get() = 0; }

        // Propagate CS lines after wiring is complete
        self.cs0_line.set(false); // CS0 active by default when CR2 = 0
        self.cs1_line.set(true);  // CS1 inactive
        self.irq.set(false);
    }
}

impl SysBusDeviceImpl for SpiState {}

impl SpiState {
    unsafe fn init(mut this: ParentInit<Self>) {
        static SPI_OPS: MemoryRegionOps<SpiState> = MemoryRegionOpsBuilder::<SpiState>::new()
            .read(&SpiState::read)
            .write(&SpiState::write)
            .little_endian()
            .impl_sizes(4, 4)
            .build();

        MemoryRegion::init_io(
            &mut uninit_field_mut!(*this, mmio),
            &SPI_OPS,
            "spi",
            0x1000,
        );

        uninit_field_mut!(*this, cr1).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, cr2).write(UnsafeCell::new(0));
        uninit_field_mut!(*this, sr).write(UnsafeCell::new(0x0000_0002));
        uninit_field_mut!(*this, dr).write(UnsafeCell::new(0));
    }

    fn post_init(&self) {
        self.init_mmio(&self.mmio);
        self.init_irq(&self.irq);
        self.init_irq(&self.cs0_line);
        self.init_irq(&self.cs1_line);

        let cs = self.cr2_val() & 0x03;
        if cs == 0 {
            self.cs0_line.set(false);
            self.cs1_line.set(true);
        } else if cs == 1 {
            self.cs0_line.set(true);
            self.cs1_line.set(false);
        } else {
            self.cs0_line.set(true);
            self.cs1_line.set(true);
        }

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

impl SpiState {
    fn cr1_val(&self) -> u32 {
        unsafe { *self.cr1.get() }
    }

    fn set_cr1(&self, val: u32) {
        unsafe { *self.cr1.get() = val; }
    }

    fn cr2_val(&self) -> u32 {
        unsafe { *self.cr2.get() }
    }

    fn set_cr2(&self, val: u32) {
        unsafe { *self.cr2.get() = val; }
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

    fn update_spi(&self) {
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
            0x04 => self.cr2_val() as u64,
            0x08 => self.sr_val() as u64,
            0x0C => {
                let data = self.dr_val();
                self.set_sr(self.sr_val() & !(1 << 0));
                self.update_spi();
                data as u64
            }
            _ => 0,
        }
    }

    fn write(&self, offset: hwaddr, value: u64, _size: u32) {
        let val32 = value as u32;
        match offset {
            0x00 => {
                self.set_cr1(val32);
                self.update_spi();
            }
            0x04 => {
                let old_cs = self.cr2_val() & 0x03;
                let new_cs = val32 & 0x03;
                if new_cs != old_cs {
                    if new_cs == 0 {
                        self.cs0_line.set(false);
                        self.cs1_line.set(true);
                    } else if new_cs == 1 {
                        self.cs0_line.set(true);
                        self.cs1_line.set(false);
                    } else {
                        self.cs0_line.set(true);
                        self.cs1_line.set(true);
                    }
                }
                self.set_cr2(val32);
                self.update_spi();
            }
            0x08 => {
                if (val32 & (1 << 4)) != 0 {
                    self.set_sr(self.sr_val() & !(1 << 4));
                    self.update_spi();
                }
            }
            0x0C => {
                let sr_before = self.sr_val();
                if (sr_before & (1 << 0)) != 0 {
                    self.set_sr(sr_before | (1 << 4));
                }
             self.set_sr(self.sr_val() & !(1 << 1));
                let flash_data = unsafe { ssi_transfer(self.ssi_bus_ptr(), val32) };
                self.set_dr(flash_data);
                self.set_sr(self.sr_val() | (1 << 0));
                self.set_sr(self.sr_val() | (1 << 1));
                self.update_spi();
            }
            _ => {}
        }
    }
}