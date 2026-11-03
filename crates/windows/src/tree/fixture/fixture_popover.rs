use super::LocalFixture;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWL_STYLE, GWLP_HWNDPARENT, GetWindowLongPtrW, GetWindowLongW, SetWindowLongPtrW,
    SetWindowLongW, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};

pub(crate) struct PopoverFixture {
    popup: LocalFixture,
    owner: LocalFixture,
}

impl PopoverFixture {
    pub(crate) fn create() -> Result<Self, String> {
        let owner = LocalFixture::create()?;
        let (left, top) = owner.origin();
        let popup = LocalFixture::create_at(left + 40, top + 40)?;
        let hwnd = popup.handle() as _;
        unsafe {
            SetWindowLongW(hwnd, GWL_STYLE, (WS_POPUP | WS_VISIBLE) as i32);
            SetWindowLongW(hwnd, GWL_EXSTYLE, WS_EX_TOOLWINDOW as i32);
            SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, owner.handle());
            if GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) != owner.handle()
                || GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_POPUP == 0
                || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0
            {
                return Err("the fixture did not become an owned tool popup".into());
            }
        }
        Ok(Self { popup, owner })
    }

    pub(crate) fn owner_handle(&self) -> isize {
        self.owner.handle()
    }

    pub(crate) fn popup_handle(&self) -> isize {
        self.popup.handle()
    }
}
