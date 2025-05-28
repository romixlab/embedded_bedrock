SECTIONS
{
  .counters_ram (INFO) :
  {
    *(.cnt_ram.*);
    __RAM_COUNTER_END_MARKER_END = .;

    /* Symbols that aren't referenced by the program and */
    /* should be placed at the end of the section */
    /*KEEP(*(.cnt.end .cnt.end.*));*/
  }
  
  .counters_bkp (INFO) :
  {
    *(.cnt_bkp.*);
    __BKP_COUNTER_END_MARKER_END = .;
  }
}