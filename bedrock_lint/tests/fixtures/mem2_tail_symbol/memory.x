MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 256K
  RAM : ORIGIN = 0x20000000, LENGTH = 144K
}

SECTIONS
{
  .cnt_bkp_buffer (NOLOAD) : ALIGN(4)
  {
    *(.cnt_bkp_buffer .cnt_bkp_buffer.*);
    . = ALIGN(4);
  } > BKPSRAM
}
