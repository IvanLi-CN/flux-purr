INCLUDE "ram-memory.x"
INCLUDE "exception.x"
REGION_ALIAS("ROTEXT", iram_seg);
REGION_ALIAS("RWTEXT", iram_seg);
REGION_ALIAS("RODATA", dram_seg);
/* Keep initialized data and the stack in the lower DRAM window.  The
   dedicated dram2 section below is reserved for large no-init buffers. */
REGION_ALIAS("RWDATA", dram_seg);
REGION_ALIAS("RTC_FAST_RWTEXT", rtc_fast_seg);
REGION_ALIAS("RTC_FAST_RWDATA", rtc_fast_seg);
INCLUDE "esp32s3.x"
INCLUDE "hal-defaults.x"
SECTIONS {
  INCLUDE "rwtext.x"
  INCLUDE "rwdata.x"
}
INCLUDE "rodata.x"
INCLUDE "text.x"
INCLUDE "rtc_fast.x"
INCLUDE "rtc_slow.x"
INCLUDE "dram2.x"
INCLUDE "stack.x"
INCLUDE "metadata.x"
INCLUDE "eh_frame.x"
