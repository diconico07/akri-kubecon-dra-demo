use tinyqoi::Qoi;

pub fn get_pause_small() -> Qoi<'static> {
    let data = include_bytes!("../icons/pause-small.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_pause() -> Qoi<'static> {
    let data = include_bytes!("../icons/pause.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_play_small() -> Qoi<'static> {
    let data = include_bytes!("../icons/play-small.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_play() -> Qoi<'static> {
    let data = include_bytes!("../icons/play.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_trash_small() -> Qoi<'static> {
    let data = include_bytes!("../icons/trash-small.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_trash_full() -> Qoi<'static> {
    let data = include_bytes!("../icons/trash-full.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_print_small() -> Qoi<'static> {
    let data = include_bytes!("../icons/print-small.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_print_nok_small() -> Qoi<'static> {
    let data = include_bytes!("../icons/print-nok-small.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_wireless_ok_small() -> Qoi<'static> {
    let data = include_bytes!("../icons/wireless-ok-small.qoi");
    Qoi::new(data).unwrap()
}

pub fn get_wireless_nok_small() -> Qoi<'static> {
    let data = include_bytes!("../icons/wireless-nok-small.qoi");
    Qoi::new(data).unwrap()
}
