let stm32_list = ["C011D6","C011F4","C011F6","C011J4","C011J6","C031C4","C031C6","C031F4","C031F6","C031G4","C031G6","C031K4","C031K6","C051C6","C051C8","C051D8","C051F6","C051F8","C051G6","C051G8","C051K6","C051K8","C071C8","C071CB","C071F8","C071FB","C071G8","C071GB","C071K8","C071KB","C071R8","C071RB","C091CB","C091CC","C091EC","C091FB","C091FC","C091GB","C091GC","C091KB","C091KC","C091RB","C091RC","C092CB","C092CC","C092EC","C092FB","C092FC","C092GB","C092GC","C092KB","C092KC","C092RB","C092RC","F030C6","F030C8","F030CC","F030F4","F030K6","F030R8","F030RC","F031C4","F031C6","F031E6","F031F4","F031F6","F031G4","F031G6","F031K4","F031K6","F038C6","F038E6","F038F6","F038G6","F038K6","F042C4","F042C6","F042F4","F042F6","F042G4","F042G6","F042K4","F042K6","F042T6","F048C6","F048G6","F048T6","F051C4","F051C6","F051C8","F051K4","F051K6","F051K8","F051R4","F051R6","F051R8","F051T8","F058C8","F058R8","F058T8","F070C6","F070CB","F070F6","F070RB","F071C8","F071CB","F071RB","F071V8","F071VB","F072C8","F072CB","F072R8","F072RB","F072V8","F072VB","F078CB","F078RB","F078VB","F091CB","F091CC","F091RB","F091RC","F091VB","F091VC","F098CC","F098RC","F098VC","F100C4","F100C6","F100C8","F100CB","F100R4","F100R6","F100R8","F100RB","F100RC","F100RD","F100RE","F100V8","F100VB","F100VC","F100VD","F100VE","F100ZC","F100ZD","F100ZE","F101C4","F101C6","F101C8","F101CB","F101R4","F101R6","F101R8","F101RB","F101RC","F101RD","F101RE","F101RF","F101RG","F101T4","F101T6","F101T8","F101TB","F101V8","F101VB","F101VC","F101VD","F101VE","F101VF","F101VG","F101ZC","F101ZD","F101ZE","F101ZF","F101ZG","F102C4","F102C6","F102C8","F102CB","F102R4","F102R6","F102R8","F102RB","F103C4","F103C6","F103C8","F103CB","F103R4","F103R6","F103R8","F103RB","F103RC","F103RD","F103RE","F103RF","F103RG","F103T4","F103T6","F103T8","F103TB","F103V8","F103VB","F103VC","F103VD","F103VE","F103VF","F103VG","F103ZC","F103ZD","F103ZE","F103ZF","F103ZG","F105R8","F105RB","F105RC","F105V8","F105VB","F105VC","F107RB","F107RC","F107VB","F107VC","F205RB","F205RC","F205RE","F205RF","F205RG","F205VB","F205VC","F205VE","F205VF","F205VG","F205ZC","F205ZE","F205ZF","F205ZG","F207IC","F207IE","F207IF","F207IG","F207VC","F207VE","F207VF","F207VG","F207ZC","F207ZE","F207ZF","F207ZG","F215RE","F215RG","F215VE","F215VG","F215ZE","F215ZG","F217IE","F217IG","F217VE","F217VG","F217ZE","F217ZG","F301C6","F301C8","F301K6","F301K8","F301R6","F301R8","F302C6","F302C8","F302CB","F302CC","F302K6","F302K8","F302R6","F302R8","F302RB","F302RC","F302RD","F302RE","F302VB","F302VC","F302VD","F302VE","F302ZD","F302ZE","F303C6","F303C8","F303CB","F303CC","F303K6","F303K8","F303R6","F303R8","F303RB","F303RC","F303RD","F303RE","F303VB","F303VC","F303VD","F303VE","F303ZD","F303ZE","F318C8","F318K8","F328C8","F334C4","F334C6","F334C8","F334K4","F334K6","F334K8","F334R6","F334R8","F358CC","F358RC","F358VC","F373C8","F373CB","F373CC","F373R8","F373RB","F373RC","F373V8","F373VB","F373VC","F378CC","F378RC","F378VC","F398VE","F401CB","F401CC","F401CD","F401CE","F401RB","F401RC","F401RD","F401RE","F401VB","F401VC","F401VD","F401VE","F405OE","F405OG","F405RG","F405VG","F405ZG","F407IE","F407IG","F407VE","F407VG","F407ZE","F407ZG","F410C8","F410CB","F410R8","F410RB","F410T8","F410TB","F411CC","F411CE","F411RC","F411RE","F411VC","F411VE","F412CE","F412CG","F412RE","F412RG","F412VE","F412VG","F412ZE","F412ZG","F413CG","F413CH","F413MG","F413MH","F413RG","F413RH","F413VG","F413VH","F413ZG","F413ZH","F415OG","F415RG","F415VG","F415ZG","F417IE","F417IG","F417VE","F417VG","F417ZE","F417ZG","F423CH","F423MH","F423RH","F423VH","F423ZH","F427AG","F427AI","F427IG","F427II","F427VG","F427VI","F427ZG","F427ZI","F429AG","F429AI","F429BE","F429BG","F429BI","F429IE","F429IG","F429II","F429NE","F429NG","F429NI","F429VE","F429VG","F429VI","F429ZE","F429ZG","F429ZI","F437AI","F437IG","F437II","F437VG","F437VI","F437ZG","F437ZI","F439AI","F439BG","F439BI","F439IG","F439II","F439NG","F439NI","F439VG","F439VI","F439ZG","F439ZI","F446MC","F446ME","F446RC","F446RE","F446VC","F446VE","F446ZC","F446ZE","F469AE","F469AG","F469AI","F469BE","F469BG","F469BI","F469IE","F469IG","F469II","F469NE","F469NG","F469NI","F469VE","F469VG","F469VI","F469ZE","F469ZG","F469ZI","F479AG","F479AI","F479BG","F479BI","F479IG","F479II","F479NG","F479NI","F479VG","F479VI","F479ZG","F479ZI","F722IC","F722IE","F722RC","F722RE","F722VC","F722VE","F722ZC","F722ZE","F723IC","F723IE","F723VC","F723VE","F723ZC","F723ZE","F730I8","F730R8","F730V8","F730Z8","F732IE","F732RE","F732VE","F732ZE","F733IE","F733VE","F733ZE","F745IE","F745IG","F745VE","F745VG","F745ZE","F745ZG","F746BE","F746BG","F746IE","F746IG","F746NE","F746NG","F746VE","F746VG","F746ZE","F746ZG","F750N8","F750V8","F750Z8","F756BG","F756IG","F756NG","F756VG","F756ZG","F765BG","F765BI","F765IG","F765II","F765NG","F765NI","F765VG","F765VI","F765ZG","F765ZI","F767BG","F767BI","F767IG","F767II","F767NG","F767NI","F767VG","F767VI","F767ZG","F767ZI","F768AI","F769AG","F769AI","F769BG","F769BI","F769IG","F769II","F769NG","F769NI","F777BI","F777II","F777NI","F777VI","F777ZI","F778AI","F779AI","F779BI","F779II","F779NI","G030C6","G030C8","G030F6","G030J6","G030K6","G030K8","G031C4","G031C6","G031C8","G031F4","G031F6","G031F8","G031G4","G031G6","G031G8","G031J4","G031J6","G031K4","G031K6","G031K8","G031Y8","G041C6","G041C8","G041F6","G041F8","G041G6","G041G8","G041J6","G041K6","G041K8","G041Y8","G050C6","G050C8","G050F6","G050K6","G050K8","G051C6","G051C8","G051F6","G051F8","G051G6","G051G8","G051K6","G051K8","G061C6","G061C8","G061F6","G061F8","G061G6","G061G8","G061K6","G061K8","G070CB","G070KB","G070RB","G071C6","G071C8","G071CB","G071EB","G071G6","G071G8","G071GB","G071K6","G071K8","G071KB","G071R6","G071R8","G071RB","G081CB","G081EB","G081GB","G081KB","G081RB","G0B0CE","G0B0KE","G0B0RE","G0B0VE","G0B1CB","G0B1CC","G0B1CE","G0B1KB","G0B1KC","G0B1KE","G0B1MB","G0B1MC","G0B1ME","G0B1NE","G0B1RB","G0B1RC","G0B1RE","G0B1VB","G0B1VC","G0B1VE","G0C1CC","G0C1CE","G0C1KC","G0C1KE","G0C1MC","G0C1ME","G0C1NE","G0C1RC","G0C1RE","G0C1VC","G0C1VE","G431C6","G431C8","G431CB","G431K6","G431K8","G431KB","G431M6","G431M8","G431MB","G431R6","G431R8","G431RB","G431V6","G431V8","G431VB","G441CB","G441KB","G441MB","G441RB","G441VB","G471CC","G471CE","G471MC","G471ME","G471QC","G471QE","G471RC","G471RE","G471VC","G471VE","G473CB","G473CC","G473CE","G473MB","G473MC","G473ME","G473PB","G473PC","G473PE","G473QB","G473QC","G473QE","G473RB","G473RC","G473RE","G473VB","G473VC","G473VE","G474CB","G474CC","G474CE","G474MB","G474MC","G474ME","G474PB","G474PC","G474PE","G474QB","G474QC","G474QE","G474RB","G474RC","G474RE","G474VB","G474VC","G474VE","G483CE","G483ME","G483PE","G483QE","G483RE","G483VE","G484CE","G484ME","G484PE","G484QE","G484RE","G484VE","G491CC","G491CE","G491KC","G491KE","G491MC","G491ME","G491RC","G491RE","G491VC","G491VE","G4A1CE","G4A1KE","G4A1ME","G4A1RE","G4A1VE","H503CB","H503EB","H503KB","H503RB","H523CC","H523CE","H523HE","H523RC","H523RE","H523VC","H523VE","H523ZC","H523ZE","H533CE","H533HE","H533RE","H533VE","H533ZE","H562AG","H562AI","H562IG","H562II","H562RG","H562RI","H562VG","H562VI","H562ZG","H562ZI","H563AG","H563AI","H563IG","H563II","H563MI","H563RG","H563RI","H563VG","H563VI","H563ZG","H563ZI","H573AI","H573II","H573MI","H573RI","H573VI","H573ZI","H723VE","H723VG","H723ZE","H723ZG","H725AE","H725AG","H725IE","H725IG","H725RE","H725RG","H725VE","H725VG","H725ZE","H725ZG","H730AB","H730IB","H730VB","H730ZB","H733VG","H733ZG","H735AG","H735IG","H735RG","H735VG","H735ZG","H742AG","H742AI","H742BG","H742BI","H742IG","H742II","H742VG","H742VI","H742XG","H742XI","H742ZG","H742ZI","H743AG","H743AI","H743BG","H743BI","H743IG","H743II","H743VG","H743VI","H743XG","H743XI","H743ZG","H743ZI","H745BG","H745BI","H745IG","H745II","H745XG","H745XI","H745ZG","H745ZI","H747AG","H747AI","H747BG","H747BI","H747IG","H747II","H747XG","H747XI","H747ZI","H750IB","H750VB","H750XB","H750ZB","H753AI","H753BI","H753II","H753VI","H753XI","H753ZI","H755BI","H755II","H755XI","H755ZI","H757AI","H757BI","H757II","H757XI","H757ZI","H7A3AG","H7A3AI","H7A3IG","H7A3II","H7A3LG","H7A3LI","H7A3NG","H7A3NI","H7A3QI","H7A3RG","H7A3RI","H7A3VG","H7A3VI","H7A3ZG","H7A3ZI","H7B0AB","H7B0IB","H7B0RB","H7B0VB","H7B0ZB","H7B3AI","H7B3II","H7B3LI","H7B3NI","H7B3QI","H7B3RI","H7B3VI","H7B3ZI","H7R3A8","H7R3I8","H7R3L8","H7R3R8","H7R3V8","H7R3Z8","H7R7A8","H7R7I8","H7R7L8","H7R7Z8","H7S3A8","H7S3I8","H7S3L8","H7S3R8","H7S3V8","H7S3Z8","H7S7A8","H7S7I8","H7S7L8","H7S7Z8","L010C6","L010F4","L010K4","L010K8","L010R8","L010RB","L011D3","L011D4","L011E3","L011E4","L011F3","L011F4","L011G3","L011G4","L011K3","L011K4","L021D4","L021F4","L021G4","L021K4","L031C4","L031C6","L031E4","L031E6","L031F4","L031F6","L031G4","L031G6","L031K4","L031K6","L041C4","L041C6","L041E6","L041F6","L041G6","L041K6","L051C6","L051C8","L051K6","L051K8","L051R6","L051R8","L051T6","L051T8","L052C6","L052C8","L052K6","L052K8","L052R6","L052R8","L052T6","L052T8","L053C6","L053C8","L053R6","L053R8","L062C8","L062K8","L063C8","L063R8","L071C8","L071CB","L071CZ","L071K8","L071KB","L071KZ","L071RB","L071RZ","L071V8","L071VB","L071VZ","L072CB","L072CZ","L072KB","L072KZ","L072RB","L072RZ","L072V8","L072VB","L072VZ","L073CB","L073CZ","L073RB","L073RZ","L073V8","L073VB","L073VZ","L081CB","L081CZ","L081KZ","L082CZ","L082KB","L082KZ","L083CB","L083CZ","L083RB","L083RZ","L083V8","L083VB","L083VZ","L100C6-A","L100C6","L100R8-A","L100R8","L100RB-A","L100RB","L100RC","L151C6-A","L151C6","L151C8-A","L151C8","L151CB-A","L151CB","L151CC","L151QC","L151QD","L151QE","L151R6-A","L151R6","L151R8-A","L151R8","L151RB-A","L151RB","L151RC-A","L151RC","L151RD","L151RE","L151UC","L151V8-A","L151V8","L151VB-A","L151VB","L151VC-A","L151VC","L151VD-X","L151VD","L151VE","L151ZC","L151ZD","L151ZE","L152C6-A","L152C6","L152C8-A","L152C8","L152CB-A","L152CB","L152CC","L152QC","L152QD","L152QE","L152R6-A","L152R6","L152R8-A","L152R8","L152RB-A","L152RB","L152RC-A","L152RC","L152RD","L152RE","L152UC","L152V8-A","L152V8","L152VB-A","L152VB","L152VC-A","L152VC","L152VD-X","L152VD","L152VE","L152ZC","L152ZD","L152ZE","L162QC","L162QD","L162RC-A","L162RC","L162RD","L162RE","L162VC-A","L162VC","L162VD-X","L162VD","L162VE","L162ZC","L162ZD","L162ZE","L412C8","L412CB","L412K8","L412KB","L412R8","L412RB","L412T8","L412TB","L422CB","L422KB","L422RB","L422TB","L431CB","L431CC","L431KB","L431KC","L431RB","L431RC","L431VC","L432KB","L432KC","L433CB","L433CC","L433RB","L433RC","L433VC","L442KC","L443CC","L443RC","L443VC","L451CC","L451CE","L451RC","L451RE","L451VC","L451VE","L452CC","L452CE","L452RC","L452RE","L452VC","L452VE","L462CE","L462RE","L462VE","L471QE","L471QG","L471RE","L471RG","L471VE","L471VG","L471ZE","L471ZG","L475RC","L475RE","L475RG","L475VC","L475VE","L475VG","L476JE","L476JG","L476ME","L476MG","L476QE","L476QG","L476RC","L476RE","L476RG","L476VC","L476VE","L476VG","L476ZE","L476ZG","L486JG","L486QG","L486RG","L486VG","L486ZG","L496AE","L496AG","L496QE","L496QG","L496RE","L496RG","L496VE","L496VG","L496WG","L496ZE","L496ZG","L4A6AG","L4A6QG","L4A6RG","L4A6VG","L4A6ZG","L4P5AE","L4P5AG","L4P5CE","L4P5CG","L4P5QE","L4P5QG","L4P5RE","L4P5RG","L4P5VE","L4P5VG","L4P5ZE","L4P5ZG","L4Q5AG","L4Q5CG","L4Q5QG","L4Q5RG","L4Q5VG","L4Q5ZG","L4R5AG","L4R5AI","L4R5QG","L4R5QI","L4R5VG","L4R5VI","L4R5ZG","L4R5ZI","L4R7AI","L4R7VI","L4R7ZI","L4R9AG","L4R9AI","L4R9VG","L4R9VI","L4R9ZG","L4R9ZI","L4S5AI","L4S5QI","L4S5VI","L4S5ZI","L4S7AI","L4S7VI","L4S7ZI","L4S9AI","L4S9VI","L4S9ZI","L552CC","L552CE","L552ME","L552QC","L552QE","L552RC","L552RE","L552VC","L552VE","L552ZC","L552ZE","L562CE","L562ME","L562QE","L562RE","L562VE","L562ZE","U031C6","U031C8","U031F4","U031F6","U031F8","U031G6","U031G8","U031K4","U031K6","U031K8","U031R6","U031R8","U073C8","U073CB","U073CC","U073H8","U073HB","U073HC","U073K8","U073KB","U073KC","U073M8","U073MB","U073MC","U073R8","U073RB","U073RC","U083CC","U083HC","U083KC","U083MC","U083RC","U535CB","U535CC","U535CE","U535JE","U535NC","U535NE","U535RB","U535RC","U535RE","U535VC","U535VE","U545CE","U545JE","U545NE","U545RE","U545VE","U575AG","U575AI","U575CG","U575CI","U575OG","U575OI","U575QG","U575QI","U575RG","U575RI","U575VG","U575VI","U575ZG","U575ZI","U585AI","U585CI","U585OI","U585QI","U585RI","U585VI","U585ZI","U595AI","U595AJ","U595QI","U595QJ","U595RI","U595RJ","U595VI","U595VJ","U595ZI","U595ZJ","U599BJ","U599NI","U599NJ","U599VI","U599VJ","U599ZI","U599ZJ","U5A5AJ","U5A5QI","U5A5QJ","U5A5RJ","U5A5VJ","U5A5ZJ","U5A9BJ","U5A9NJ","U5A9VJ","U5A9ZJ","U5F7VI","U5F7VJ","U5F9BJ","U5F9NJ","U5F9VI","U5F9VJ","U5F9ZI","U5F9ZJ","U5G7VJ","U5G9BJ","U5G9NJ","U5G9VJ","U5G9ZJ","WB10CC","WB15CC","WB30CE","WB35CC","WB35CE","WB50CG","WB55CC","WB55CE","WB55CG","WB55RC","WB55RE","WB55RG","WB55VC","WB55VE","WB55VG","WB55VY","WBA50KE","WBA50KG","WBA52CE","WBA52CG","WBA52KE","WBA52KG","WBA54CE","WBA54CG","WBA54KE","WBA54KG","WBA55CE","WBA55CG","WBA55HE","WBA55HG","WBA55UE","WBA55UG","WL54CC","WL54JC","WL55CC","WL55JC","WLE4C8","WLE4CB","WLE4CC","WLE4J8","WLE4JB","WLE4JC","WLE5C8","WLE5CB","WLE5CC","WLE5J8","WLE5JB","WLE5JC"];
let manual_targets = #{
    rp2040: #{
        arch: "arm",
        rust_target: "thumbv6m-none-eabi",
	    flash_start: "0x10000100",
        flash_size: "2048K - 0x100",
        ram_start: "0x20000000",
        ram_size: "264K",
        probe_chip: "RP2040"
    },

    nrf52833: #{
        arch: "arm",
        rust_target: "thumbv7em-none-eabihf",
        probe_chip: "nRF52833_xxAA",
	    flash_start: "0x00000000",
        flash_size: "512K",
        ram_start: "0x20000000",
        ram_size: "128K"
    },

    nrf52840: #{
        arch: "arm",
        rust_target: "thumbv7em-none-eabihf",
        probe_chip: "nRF52840_xxAA",
	    flash_start: "0x00000000",
        flash_size: "1024K",
        ram_start: "0x20000000",
        ram_size: "256K"
    },

    nrf9160: #{
        arch: "arm",
        rust_target: "thumbv8m.main-none-eabihf",
        probe_chip: "nRF9160_xxAA",
        flash_start: "0x00000000",
        flash_size: "1024K",
        ram_start: "0x20010000",
        ram_size: "192K",
    },

    nrf9151: #{
        arch: "arm",
        rust_target: "thumbv8m.main-none-eabihf",
        probe_chip: "nRF9160_xxAA",
        flash_start: "0x00000000",
        flash_size: "1024K",
        ram_start: "0x20010000",
        ram_size: "192K",
    },

    rp2350: #{
        arch: "arm",
        rust_target: "thumbv8m.main-none-eabihf",
        flash_start: "0x10000100",
        flash_size: "4096K - 0x100",
        ram_start: "0x20000000",
        ram_size: "520K",
        probe_chip: "RP235x"
    },

    rp2354: #{
        arch: "arm",
        rust_target: "thumbv8m.main-none-eabihf",
        flash_start: "0x10000100",
        flash_size: "2048K - 0x100",
        ram_start: "0x20000000",
        ram_size: "520K",
        probe_chip: "RP235x"
    },

    esp32c3: #{
        arch: "riscv",
        rust_target: "riscv32imc-unknown-none-elf",
        probe_chip: "esp32c3"
    },
};

let chip = variable::prompt("Which MCU to use?", "STM32H725IG");
chip.make_lower();
variable::set("chip", chip);
let counters_info = if variable::get("use_counters") {
    prompt_counters_info(chip)
} else {
    ()
};

if chip.starts_with("stm32") {
    chip.make_upper();
    let series = chip[5..=6];

    if !stm32_list.contains(chip[5..]) {
        show_similar_stm32_and_abort(chip, stm32_list);
    }
    print("↓ Download " + chip + " information from embassy-rs/stm32-data-generated repo using curl?");
    print("Decline if no internet or for other reasons, will use safe defaults then.");
    let download_ok = variable::prompt("Proceed with download?", "yes", ["no", "yes"]);
    if download_ok == "yes" {
        let mcu_info = download_stm32_info(chip);
        if type_of(mcu_info) == "()" {
            abort("\x1b[31mSomething went wrong with the download.");
        } else {
            generate_memory_x(mcu_info, counters_info);
            generate_readme_md_extended(mcu_info);
            prompt_smps_configuration(mcu_info);
        }
    } else {
        generate_default_memory_x_stm32();
        generate_readme_md();
    }

    variable::set("arch", "arm");
    let rust_target = switch series {
        "C0" | "F0" | "G0" | "L0" | "U0" => "thumbv6m-none-eabi",
        "F1" | "F2" | "L1" | "WL" => "thumbv7m-none-eabi",
        "F3" | "F7" | "H7" => "thumbv7em-none-eabihf",
        "WB" if chip[7] != 'A' => "thumbv7em-none-eabihf",
        "WB" if chip[7] == 'A' => "thumbv8m.main-none-eabihf",
        "F4" | "G4" | "L4" => "thumbv7em-none-eabi",
        "H5" | "L5" | "U5" => "thumbv8m.main-none-eabihf",
    };
    variable::set("rust_target", rust_target);
    variable::set("probe_chip", chip);
} else {
    let target_properties = manual_targets.get(target);
    for key in target_properties.keys() {
        variable::set(key, target_properties.get(key));
    }
    generate_readme_md();
}
generate_config_toml(counters_info);
generate_rust_toolchain_toml();

fn show_similar_stm32_and_abort(chip, stm32_list) {
    let similar = [];
    let stop = false;
    for i in 1..chip.len() - 5 {
        if stop {
            break;
        }
        for s in stm32_list {
            if s.starts_with(chip[5..-i]) {
                similar.push("STM32" + s);
                stop = true;
            }
        }
    }
    if similar.is_empty() {
        abort("Unknown MCU");
    } else {
        abort("Unknown MCU, maybe you meant one of: " + similar + "?");
    }
}

fn download_stm32_info(chip) {
    let json_url = "https://raw.githubusercontent.com/embassy-rs/stm32-data-generated/main/data/chips/" + chip + ".json";
    let mcu_info = system::command("curl", ["-s", json_url]);
    if mcu_info.is_empty() {
        ()
    } else {
        parse_json(mcu_info)
    }
}

fn generate_memory_x(mcu_info, counters_info) {
    let regions = [];
    for m in mcu_info.memory[0] {
        regions.push(#{
            origin: m.address,
            length: m.size,
            name: m.name,
            kind: m.kind,
            comment_out: false,
            selected_ram: false,
            collect_sections: "",
            comment: ""
        });
    }

    let ram_found = false;
    for i in 0..regions.len() {
        if regions[i].name == "AXISRAM" {
            regions[i].selected_ram = true;
            regions[i].comment_out = true;
            ram_found = true;
            break;
        }
    }
    if !ram_found {
        for i in 0..regions.len() {
            if regions[i].kind == "ram" {
                regions[i].selected_ram = true;
                regions[i].comment_out = true;
                break;
            }
        }
    }
    for i in 0..regions.len() {
        if regions[i].kind == "flash" {
            regions[i].name = "FLASH";
            break;
        }
    }

    if counters_info.use_tamp {
        let address = mcu_info.cores[0].peripherals.find(|p| p.name == "TAMP")?.address;
        let address = if type_of(address) == "()" {
            0xFFFF_FFFF
        } else {
            address + 0x100
        };
        regions.push(#{
            origin: address,
            length: counters_info.bkp_size,
            name: "TAMP_BKP_REGS",
            kind: "reg",
            comment_out: false,
            selected_ram: false,
            collect_sections: ".cnt_bkp_buffer",
            comment: "TODO: Check that this address is correct"
        });
    }
    if counters_info.use_rtc {
        let address = mcu_info.cores[0].peripherals.find(|p| p.name == "RCC")?.address;
        let address = if type_of(address) == "()" {
            0xFFFF_FFFF
        } else {
            address + 0x50
        };
        regions.push(#{
            origin: address,
            length: counters_info.bkp_size,
            name: "RCC_BKP_REGS",
            kind: "reg",
            comment_out: false,
            selected_ram: false,
            collect_sections: ".cnt_bkp_buffer",
            comment: "TODO: Check that this address is correct"
        });
    }

    let out = [];
    out += generate_linker_regions(regions);
    out += generate_linker_sections(regions);
    out += generate_linker_consts(regions);

    file::write("memory.x", out);
}

fn generate_default_memory_x_stm32() {
    let regions = [];
    regions.push(#{
        origin: 0x2000_0000,
        length: 16_384,
        name: "RAM",
        kind: "ram",
        comment_out: false,
        selected_ram: true,
        comment: "TODO: Check that this address and size are correct (they are likely not)"
    });
    regions.push(#{
        origin: 0x0800_0000,
        length: 16_384,
        name: "FLASH",
        kind: "flash",
        comment_out: false,
        selected_ram: true,
        comment: "TODO: Check that this address and size are correct (the are likely not)"
    });
    out += generate_linker_regions(regions);
    file::write("memory.x", out);
}

fn generate_linker_regions(regions) {
    let out = ["MEMORY", "{", "  /* FLASH and RAM are mandatory memory regions */"];
    for region in regions {
        let origin = region.origin.to_hex();
        origin.make_upper();
        let length = if region.length % 1024 == 0 {
            let length_kb = region.length / 1024;
            `${length_kb}K`
        } else {
            region.length
        };
        out.push("");
        out.push(`  /* ${region.name} (${region.kind}) */`);
        if region.name == "AXISRAM" {
            out.push("  /* TODO: Check whether AXISRAM size can be reconfigured vs ITCM (TCM_AXI_SHARED in ref. manual) */");
            out.push("  /* Using AXISRAM as main RAM to avoid potential DMA issues (vs DTCM for example), other configurations might be viable as well */");
        }
        if !region.comment.is_empty() {
            out += `  /* ${region.comment} */`;
        }
        let region_def = `  ${region.name} : ORIGIN = 0x${origin}, LENGTH = ${length}`;
        if region.comment_out {
            out.push(`  /* ${region_def} */`);
        } else {
            out.push(region_def);
        }
        if region.selected_ram {
            let region_def = `  RAM : ORIGIN = 0x${origin}, LENGTH = ${length}`;
            out.push(region_def);
        }
    }
    out.push("}");
    out
}

fn generate_linker_sections(regions) {
    let out = [];
    let count = 0;
    out.push("");
    out.push("SECTIONS {");
    for region in regions {
        if region.comment_out || region.selected_ram || region.kind == "flash" {
            continue
        }
        count += 1;
        let name_lower = region.name;
        name_lower.make_lower();
        let align = if region.name == "AXISRAM" { 8 } else { 4 };
        out.push(`  .${name_lower} (NOLOAD) : ALIGN(${align}) {`);
        if region.collect_sections.is_empty() {
            out.push(`    *(.${name_lower} .${name_lower}.*);`);
        } else {
            out.push(`    *(${region.collect_sections});`);
        }
        out.push(`    . = ALIGN(${align});`);
        out.push(`  } > ${region.name}`);
    }
    out.push("};");
    if count > 0 { out } else { [] }
}

fn generate_linker_consts(regions) {
    let out = [];
    let count = 0;
    out.push("");
    out.push("/* Helper constants to init additional memory banks to zero (for example if using with StaticCell) */");
    out.push("/* TODO: Check whether RAM banks need to be explicitly enabled before use */");
    for region in regions {
        if region.comment_out || region.selected_ram || region.kind == "flash" {
            continue
        }
        let name_lower = region.name;
        name_lower.make_lower();
        out.push(`_${name_lower}_start = ORIGIN(${region.name});`);
        out.push(`_${name_lower}_end = ORIGIN(${region.name}) + LENGTH(${region.name});`);
        out.push("");
    }
    if count > 0 { out } else { [] }
}

fn generate_config_toml(counters_info) {
    let out = [];
    let rust_target = variable::get("rust_target");
    out += `[target.${rust_target}]`;
    let probe_chip = variable::get("probe_chip");
    if probe_chip.contains("nrf9151") {
        out += `runner = "probe-rs run --chip ${probe_chip} --allow-erase-all"`;
    } else {
        out += `runner = "probe-rs run --chip ${probe_chip}"`;
    }
    out += "";

    out += "[build]";
    out += `target = "${rust_target}"`;
    out += "";

    out += "[env]";
    out += `DEFMT_LOG = "${variable::get("defmt_log")}" # By default, only error level messages are emitted`;

    let defmt_rtt_buffer_size = variable::get("defmt_rtt_buffer_size");
    let comment = "avoid loosing output if non-blocking, e.g., during init when a lot of output is generated";
    if defmt_rtt_buffer_size != "1024" {
        out += `DEFMT_RTT_BUFFER_SIZE = "${defmt_rtt_buffer_size}" # ${comment}`;
    } else {
        out += `#DEFMT_RTT_BUFFER_SIZE = "4096" #${comment}`;
    }

    if variable::get("use_counters") {
        let comment = "Size of the counters buffer in RAM (words, default is 64)";
        if counters_info.ram_size != "64" {
            out += `CNT_RAM_BUFFER_SIZE_WORDS = "${counters_info.ram_size}" # ${comment}`;
        } else {
            out += `#CNT_RAM_BUFFER_SIZE_WORDS = "64" #${comment}`;
        }

        let comment = "Size of the counters buffer in BKPRAM (words, default is 0)";
        if counters_info.bkp_size != "0" {
            let size_words = counters_info.bkp_size / 4;
            out += `CNT_BKP_BUFFER_SIZE_WORDS = "${size_words}" # ${comment}`;
        } else {
            out += `#CNT_BKP_BUFFER_SIZE_WORDS = "64" #${comment}`;
        }
        out += "";
    }

    out += "# Minimize build size";
    let build_core = variable::get("build_core");
    let panic_immediate_abort = variable::get("panic_immediate_abort");
    let comment_out = if !build_core && !panic_immediate_abort { '#' } else { "" };
    out += `${comment_out}[unstable]`;
    let comment_out = if !build_core { '#' } else { "" };
    out += `${comment_out}build-std = ["core"]`;
    let comment_out = if !panic_immediate_abort { '#' } else { "" };
    out += `${comment_out}build-std-features = ["panic_immediate_abort"] # for even smaller size`;

    file::write(".cargo/config.toml", out);
}

fn prompt_counters_info(chip) {
    let ram_size = variable::prompt("Size of the counters buffer in RAM (words)", "64");

    let bkp_size = 0;
    let use_tamp = false;
    let use_rtc = false;
    let use_bkpram = false;

    if chip.starts_with("stm32g0") {
        use_tamp = variable::prompt("Use TAMP backup registers as non-volatile counters? (5 words)", "yes", ["yes", "no"]) == "yes";
        if use_tamp {
            bkp_size = 5 * 4;
        }
    } else if chip.starts_with("stm32h7") {
        use_rtc = variable::prompt("Use RTC backup registers as non-volatile counters? (32 words)", "no", ["yes", "no"]) == "yes";
        if use_rtc {
            bkp_size = 32 * 4;
        } else {
            bkp_size = variable::prompt("Size of the counters buffer in BKPRAM (words)", "0");
            use_bkpram = true;
        }
    } else {
        bkp_size = variable::prompt("Size of the counters buffer in BKPRAM (words)", "0");
        use_bkpram = true;
    }
    variable::set("use_bkp_counters", bkp_size > 0);

    #{ ram_size: ram_size, bkp_size: bkp_size, use_tamp: use_tamp, use_rtc: use_rtc, use_bkpram: use_bkpram }
}

fn generate_readme_md() {
    let out = [];
    out += readme_md_inner();
    file::write("README.md", out);
}

fn generate_readme_md_extended(mcu_info) {
    let out = [];
    out += readme_md_inner();

    out += "";
    out += "## MCU Info";
    out += `* Family: ${mcu_info.family}`;
    out += `* Line: ${mcu_info.line}`;
    out += `* Die: ${mcu_info.die}`;
    out += `* Device ID: ${mcu_info.device_id}`;
    out += "* Packages:";
    for p in mcu_info.packages {
        out += `    * ${p.name}: ${p["package"]} (${p.pins.len()} pins)`;
    }

    out += "";
    out += "## Documentation";
    for doc in mcu_info.docs {
        out += `* [${doc.title} (${doc.type} / ${doc.name})](${doc.url})`;
    }

    file::write("README.md", out);
}

fn readme_md_inner() {
    let out = [];
    out += `# ${variable::get("project-name")}`;
    out += "";

    out += "## Environment setup";
    out += "";

    out += "## RAM linking";
    out += "";

    out
}

fn generate_rust_toolchain_toml() {
    let out = [];
    out += "[toolchain]";
    if variable::get("use_nightly") {
        print("Getting installed rustc nightly version to fix it in the rust-toolchain.toml");
        let version = system::command("rustc", ["+nightly", "--version"]);
        let date = version.split_rev(' ')[0][..-1];
        out += `channel = "nightly-${date}"`;
        out += `components = ["rustfmt", "rust-src"]`;
    } else {
        out += `components = ["rustfmt"]`;
    }
    out += `targets = ["${variable::get("rust_target")}"]`;
    file::write("rust-toolchain.toml", out);
}

fn prompt_smps_configuration(mcu_info) {
    if !any_smps_pins(mcu_info) {
        return;
    }
    let supply_config = variable::prompt(
        "\x1b[31mWARNING: Found SMPS pins, meaning that selected MCU likely contains a DC-DC supply that must be configured properly, consult schematics of your board and select the correct option",
        "DirectSMPS",
        ["Default", "LDO", "DirectSMPS", "SMPSLDO", "SMPSExternalLDO", "SMPSExternalLDOBypass", "SMPSDisabledLDOBypass"]
    );
    variable::set("supply_config", supply_config);
    if ["SMPSLDO", "SMPSExternalLDO", "SMPSExternalLDOBypass"].contains(supply_config) {
        let smps_supply_voltage = variable::prompt("Choose SMPS supply voltage", "1.8V", ["1.8V", "2.5V"]);
        variable::set("smps_supply_voltage", smps_supply_voltage);
    }
}

fn any_smps_pins(mcu_info) {
    for p in mcu_info.packages {
        for pin in p.pins {
            for signal in pin.signals {
                if signal.contains("SMPS") {
                    return true;
                }
            }
        }
    }
    false
}