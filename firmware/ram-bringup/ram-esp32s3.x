/* ESP32-S3 RAM-only linker fragment. */
PROVIDE(__pre_init = DefaultPreInit);
PROVIDE(__zero_bss = default_mem_hook);
PROVIDE(__init_data = default_mem_hook);
PROVIDE(__post_init = default_post_init);

INCLUDE exception.x

/* Keep RW text and vectors out of the physical DRAM alias used by data. */
SECTIONS {
  .rotext_dummy (NOLOAD) :
  {
    _rotext_reserved_start = .;
  } > ROTEXT
}
INSERT BEFORE .text;

SECTIONS {
  .rwdata_dummy (NOLOAD) : ALIGN(4)
  {
    . = . + SIZEOF(.rwtext) + SIZEOF(.rwtext.wifi) + SIZEOF(.vectors);
  } > RWDATA
}
INSERT BEFORE .data;

EXTERN(DefaultHandler);
INCLUDE device.x
