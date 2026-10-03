//! Which graphics APIs an executable can use, from the import-name strings in
//! its read-only data. The result is an ordered set, newest API first, so the
//! router can say "DX12 or DX11" without caring which the game defaults to.

use crate::model::GraphicsApi;

/// Lowercase needles; the haystack is compared case-insensitively.
const MARKERS: &[(GraphicsApi, &[&str])] = &[
    (GraphicsApi::Dx12, &["d3d12.dll", "d3d12createdevice"]),
    (GraphicsApi::Vulkan, &["vulkan-1.dll", "vkcreateinstance"]),
    (GraphicsApi::Dx11, &["d3d11.dll", "d3d11createdevice"]),
    (
        GraphicsApi::Dx10,
        &["d3d10.dll", "d3d10_1.dll", "d3d10createdevice"],
    ),
    (GraphicsApi::Dx9, &["d3d9.dll", "direct3dcreate9"]),
    (GraphicsApi::OpenGl, &["opengl32.dll", "wglcreatecontext"]),
];

/// Order in which APIs are listed; also the router's preference.
pub const PRIORITY: &[GraphicsApi] = &[
    GraphicsApi::Dx12,
    GraphicsApi::Vulkan,
    GraphicsApi::Dx11,
    GraphicsApi::Dx10,
    GraphicsApi::Dx9,
    GraphicsApi::OpenGl,
];

pub fn detect(bytes: &[u8]) -> Vec<GraphicsApi> {
    PRIORITY
        .iter()
        .copied()
        .filter(|api| {
            MARKERS
                .iter()
                .find(|(a, _)| a == api)
                .is_some_and(|(_, needles)| needles.iter().any(|n| contains_ci(bytes, n)))
        })
        .collect()
}

/// Case-insensitive (ASCII) substring search. `needle` must be lowercase.
pub fn contains_ci(hay: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    if needle.is_empty() || hay.len() < needle.len() {
        return needle.is_empty();
    }
    let first = needle[0];
    let first_upper = first.to_ascii_uppercase();
    let mut i = 0;
    let last_start = hay.len() - needle.len();
    while i <= last_start {
        let b = hay[i];
        if (b == first || b == first_upper) && hay[i..i + needle.len()].eq_ignore_ascii_case(needle)
        {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_apis_case_insensitively_in_priority_order() {
        let bytes = b"...D3D11.DLL...d3d12createdevice...Vulkan-1.dll...";
        assert_eq!(
            detect(bytes),
            vec![GraphicsApi::Dx12, GraphicsApi::Vulkan, GraphicsApi::Dx11]
        );
    }

    #[test]
    fn d3d11_does_not_match_d3d12_and_vice_versa() {
        assert_eq!(detect(b"xx d3d12.dll xx"), vec![GraphicsApi::Dx12]);
        assert_eq!(detect(b"xx d3d11.dll xx"), vec![GraphicsApi::Dx11]);
        assert_eq!(detect(b"xx d3d9.dll xx"), vec![GraphicsApi::Dx9]);
        assert!(detect(b"nothing here").is_empty());
    }

    #[test]
    fn contains_ci_edge_cases() {
        assert!(contains_ci(b"ABC", "abc"));
        assert!(contains_ci(b"xabc", "abc"));
        assert!(!contains_ci(b"ab", "abc"));
        assert!(contains_ci(b"", ""));
        assert!(!contains_ci(b"abd", "abc"));
    }
}
