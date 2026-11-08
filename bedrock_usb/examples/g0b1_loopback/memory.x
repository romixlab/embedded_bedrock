/* STM32G0B1RE: 512 KiB flash in two banks of 256 KiB, 144 KiB SRAM */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 512K
  RAM   : ORIGIN = 0x20000000, LENGTH = 144K
}

/* ES0548 2.2.10 "Prefetch failure when branching across flash memory banks": the code stays in bank 1, so flash
   prefetch can stay on */
ASSERT(__etext <= 0x08040000, "code reaches flash bank 2: turn prefetch off (ES0548 2.2.10) or drop this check");
