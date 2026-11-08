/* STM32G474RE: 512 KiB flash; SRAM1 (80 KiB) and SRAM2 (16 KiB) contiguous from 0x2000_0000 (DS12288 Rev 6). The
   CCM SRAM after them is left out: the example has no use for it. */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM   : ORIGIN = 0x20000000, LENGTH = 96K
}
