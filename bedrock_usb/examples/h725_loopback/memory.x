/* STM32H725IG: 1 MiB flash; the stack, .data and .bss in the 128 KiB DTCM (RM0468 Rev 3 Table 7) */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
