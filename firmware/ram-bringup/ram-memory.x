/* ESP32-S3 internal-memory-only layout for espflash --ram. */
MEMORY
{
  /* Keep the ESP32-S3 instruction cache window reserved.  ROM RAM loads
     cannot write the 0x40370000..0x40378000 cache aperture. */
  vectors_seg ( RX ) : ORIGIN = 0x40370000 + 0x8000, len = 0x400
  /* Keep the ROM USB downloader's working IRAM window clear. */
  iram_seg ( RX ) : ORIGIN = 0x40390000, len = 0x4D000
  iram_rw_seg ( RX ) : ORIGIN = 0x40390000, len = 0x4D000
  dram_seg ( RW ) : ORIGIN = 0x3FC88000, len = 0x53700
  /* The RAM image has no second-stage app loader reserving the upper DRAM
     window, so the bring-up data/stack region may use the full internal
     range through 0x3FD00000. */
  dram2_seg ( RW ) : ORIGIN = 0x3FCDB700, len = 0x1A010
  irom_seg ( RX ) : ORIGIN = 0x40370000 + 0x8000 + 0x400, len = 0x5D000
  drom_seg ( R ) : ORIGIN = 0x3FC88000, len = 0x55000
  rtc_fast_seg ( RWX ) : ORIGIN = 0x600FE000, len = 8K
  rtc_slow_seg ( RW ) : ORIGIN = 0x50000000, len = 8K
}
