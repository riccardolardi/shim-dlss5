//! Authenticode checks through WinVerifyTrust. Used only to *show* whether a
//! user-supplied file is signed and by whom; nothing is decided on it.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use shim_core::platform::{Signature, SignatureChecker};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    CERT_E_CHAINING, CERT_E_EXPIRED, CERT_E_REVOKED, CERT_E_UNTRUSTEDROOT, TRUST_E_BAD_DIGEST,
    TRUST_E_EXPLICIT_DISTRUST, TRUST_E_NOSIGNATURE, TRUST_E_PROVIDER_UNKNOWN,
    TRUST_E_SUBJECT_FORM_UNKNOWN, TRUST_E_SUBJECT_NOT_TRUSTED,
};
use windows_sys::Win32::Security::Cryptography::{
    CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE,
};
use windows_sys::Win32::Security::WinTrust::{
    WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData,
    WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
    WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
};

pub struct WinAuthenticode;

impl SignatureChecker for WinAuthenticode {
    fn check(&self, file: &Path) -> Signature {
        verify(file)
    }
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn verify(file: &Path) -> Signature {
    let path = wide(file.as_os_str());
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: path.as_ptr(),
        hFile: std::ptr::null_mut(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: std::ptr::null_mut(),
        pSIPClientData: std::ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: std::ptr::null_mut(),
        pwszURLReference: std::ptr::null_mut(),
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        dwUIContext: 0,
        pSignatureSettings: std::ptr::null_mut(),
    };
    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;

    // SAFETY: every pointer refers to a local that outlives both calls, and
    // the struct sizes are set as WinTrust requires.
    let status = unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            &mut data as *mut WINTRUST_DATA as *mut core::ffi::c_void,
        )
    };
    let signer = unsafe { signer_name(data.hWVTStateData) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: closes the state handle opened by the verify call above.
    unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            &mut data as *mut WINTRUST_DATA as *mut core::ffi::c_void,
        );
    }

    let signer_or = |fallback: &str| signer.clone().unwrap_or_else(|| fallback.to_string());
    match status {
        0 => Signature::Valid {
            signer: signer_or("unknown signer"),
        },
        TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => {
            Signature::Unsigned
        }
        TRUST_E_BAD_DIGEST => Signature::HashMismatch {
            signer: signer_or("unknown signer"),
        },
        CERT_E_UNTRUSTEDROOT
        | CERT_E_CHAINING
        | CERT_E_EXPIRED
        | CERT_E_REVOKED
        | TRUST_E_EXPLICIT_DISTRUST
        | TRUST_E_SUBJECT_NOT_TRUSTED => Signature::Unknown(format!(
            "signed by {} but the certificate is not trusted (0x{:08X})",
            signer_or("an unknown signer"),
            status as u32
        )),
        other => Signature::Unknown(format!("WinVerifyTrust returned 0x{:08X}", other as u32)),
    }
}

/// Simple display name of the leaf certificate of the first signer.
///
/// # Safety
/// `state` must be the `hWVTStateData` of a WinVerifyTrust call that has not
/// been closed yet (or null).
unsafe fn signer_name(state: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
    if state.is_null() {
        return None;
    }
    let prov = WTHelperProvDataFromStateData(state);
    if prov.is_null() {
        return None;
    }
    let sgnr = WTHelperGetProvSignerFromChain(prov, 0, 0, 0);
    if sgnr.is_null() {
        return None;
    }
    let cert = WTHelperGetProvCertFromChain(sgnr, 0);
    if cert.is_null() || (*cert).pCert.is_null() {
        return None;
    }
    let ctx = (*cert).pCert;
    let len = CertGetNameStringW(
        ctx,
        CERT_NAME_SIMPLE_DISPLAY_TYPE,
        0,
        std::ptr::null(),
        std::ptr::null_mut(),
        0,
    );
    if len <= 1 {
        return None;
    }
    let mut buf = vec![0u16; len as usize];
    let written = CertGetNameStringW(
        ctx,
        CERT_NAME_SIMPLE_DISPLAY_TYPE,
        0,
        std::ptr::null(),
        buf.as_mut_ptr(),
        len,
    );
    if written <= 1 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..written as usize - 1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_file_reads_as_unsigned_and_missing_file_as_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("plain.bin");
        std::fs::write(&file, b"not signed").unwrap();
        assert_eq!(WinAuthenticode.check(&file), Signature::Unsigned);
        assert!(matches!(
            WinAuthenticode.check(&tmp.path().join("missing.dll")),
            Signature::Unknown(_)
        ));
    }

    #[test]
    fn a_signed_system_binary_names_its_signer() {
        // notepad.exe is catalog-signed, not embedded; use a file with an
        // embedded signature that every Windows install has.
        let candidates = [
            r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
            r"C:\Program Files\nodejs\node.exe",
            r"C:\Program Files\Git\cmd\git.exe",
        ];
        let mut saw_valid = false;
        for c in candidates {
            let p = Path::new(c);
            if !p.is_file() {
                continue;
            }
            if let Signature::Valid { signer } = WinAuthenticode.check(p) {
                assert!(!signer.is_empty());
                saw_valid = true;
                break;
            }
        }
        assert!(saw_valid, "none of the candidate files verified as signed");
    }
}
