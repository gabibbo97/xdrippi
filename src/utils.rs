use std::os::fd::AsRawFd;

pub(crate) fn getsockopt<T: Sized>(socket: impl AsRawFd, level: libc::c_int, name: libc::c_int) -> Result<T, crate::Error> {
    // get option
    let mut option = std::mem::MaybeUninit::<T>::zeroed();
    let mut option_len = std::mem::size_of::<T>() as libc::socklen_t;
    let result = unsafe { libc::getsockopt(socket.as_raw_fd(), level, name, option.as_mut_ptr() as *mut _, &mut option_len as *mut _) };
    
    // check result
    if result < 0 {
        return Err(crate::Error::SocketGetOptionFailure { error: std::io::Error::last_os_error(), level, name });
    }

    // check length
    if option_len as usize != std::mem::size_of::<T>() {
        return Err(crate::Error::SocketGetOptionSizeFailure { expecting: std::mem::size_of::<T>(), received: option_len as usize });
    }

    // return the checked option
    Ok(unsafe { option.assume_init() })
}


pub(crate) fn setsockopt<T: Sized>(socket: impl AsRawFd, level: libc::c_int, name: libc::c_int, value: &T) -> Result<(), crate::Error> {
    let result = unsafe { libc::setsockopt(socket.as_raw_fd(), level, name, value as *const _ as *const libc::c_void, std::mem::size_of::<T>() as u32) };
    if result < 0 {
        Err(crate::Error::SocketSetOptionFailure { error: std::io::Error::last_os_error(), level, name })
    } else {
        Ok(())
    }
}

pub fn interface_index_to_name(interface_index: libc::c_uint) -> Option<String> {
    let mut buffer = [0_u8; libc::IF_NAMESIZE];
    let result = unsafe { libc::if_indextoname(interface_index, buffer.as_mut_ptr() as *mut _) };
    if result.is_null() {
        return None;
    }
    let if_name = std::ffi::CStr::from_bytes_until_nul(&buffer)
        .expect("ifname is not a null-terminated string")
        .to_str().expect("ifname is not an UTF8 string")
        .to_string();
    Some(if_name)
}

pub fn interface_name_to_index(interface_name: impl AsRef<str>) -> Option<libc::c_uint> {
    // TODO - path injection
    std::fs::read_to_string(format!("/sys/class/net/{}/ifindex", interface_name.as_ref()))
        .ok()
        .map(|ifindex_str| ifindex_str.trim().parse().expect("ifindex was not a number!"))
}
