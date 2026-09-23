#include <stdio.h>
#include <wchar.h>
#include <string.h>
#include <stdlib.h>

// Core Helper Functions

bool IsValidIp(const wchar_t* ipStr) {
    if (!ipStr || ipStr[0] == L'\0') return false;
    if (wcscmp(ipStr, L"0.0.0.0") == 0) return false;
    if (wcsncmp(ipStr, L"169.254.", 8) == 0) return false;

    int dots = 0;
    int currentVal = 0;
    bool hasDigits = false;

    for (int i = 0; ipStr[i] != L'\0'; i++) {
        wchar_t c = ipStr[i];
        if (c >= L'0' && c <= L'9') {
            currentVal = currentVal * 10 + (c - L'0');
            if (currentVal > 255) return false;
            hasDigits = true;
        } else if (c == L'.') {
            if (!hasDigits) return false;
            dots++;
            currentVal = 0;
            hasDigits = false;
        } else {
            return false;
        }
    }
    return (dots == 3 && hasDigits);
}

// Format string ke REG_MULTI_SZ (diakhiri double null: "192.168.180.3\0\0")
unsigned long BuildMultiSz(const wchar_t* src, wchar_t* outBuf, unsigned long maxChars) {
    if (!src || maxChars < 3) return 0;

    unsigned long len = (unsigned long)wcslen(src);
    if (len + 2 > maxChars) return 0;

    for (unsigned long i = 0; i < len; i++) {
        outBuf[i] = src[i];
    }
    outBuf[len] = L'\0';
    outBuf[len + 1] = L'\0';

    // Total size in bytes: (len + 2) * sizeof(wchar_t)
    return (len + 2) * sizeof(wchar_t);
}

// Gabungkan DNS1 dan DNS2 menjadi comma-separated "8.8.8.8,8.8.4.4"
void CombineDns(const wchar_t* dns1, const wchar_t* dns2, wchar_t* outDns, unsigned long maxChars) {
    if (!outDns || maxChars == 0) return;
    outDns[0] = L'\0';

    if (IsValidIp(dns1)) {
        wcscpy_s(outDns, maxChars, dns1);
        if (IsValidIp(dns2)) {
            wcscat_s(outDns, maxChars, L",");
            wcscat_s(outDns, maxChars, dns2);
        }
    } else if (IsValidIp(dns2)) {
        wcscpy_s(outDns, maxChars, dns2);
    }
}

// Memastikan HostName memiliki akhiran "TM" jika belum ada
void FormatHostnameWithTm(const wchar_t* inHost, wchar_t* outHost, unsigned long maxChars) {
    if (!inHost || inHost[0] == L'\0') {
        wcscpy_s(outHost, maxChars, L"PC-CLIENTTM");
        return;
    }

    wcscpy_s(outHost, maxChars, inHost);
    size_t len = wcslen(outHost);

    // Cek apakah sudah berakhiran "TM"
    bool alreadyHasTm = false;
    if (len >= 2 && outHost[len - 2] == L'T' && outHost[len - 1] == L'M') {
        alreadyHasTm = true;
    }

    if (!alreadyHasTm && len + 2 < maxChars) {
        outHost[len] = L'T';
        outHost[len + 1] = L'M';
        outHost[len + 2] = L'\0';
    }
}

// Test Runner
int main() {
    printf("====================================================\n");
    printf(" Running iSharePnp Static IP Helper Unit Tests\n");
    printf("====================================================\n\n");

    int passed = 0;
    int failed = 0;

    auto ASSERT_TRUE = [&](bool condition, const char* testName) {
        if (condition) {
            printf("[PASS] %s\n", testName);
            passed++;
        } else {
            printf("[FAIL] %s\n", testName);
            failed++;
        }
    };

    // Test 1: IP Validation
    ASSERT_TRUE(IsValidIp(L"192.168.180.3"), "Valid IP 192.168.180.3");
    ASSERT_TRUE(IsValidIp(L"255.255.255.0"), "Valid Mask 255.255.255.0");
    ASSERT_TRUE(IsValidIp(L"192.168.180.1"), "Valid Gateway 192.168.180.1");
    ASSERT_TRUE(!IsValidIp(L"0.0.0.0"), "Reject 0.0.0.0");
    ASSERT_TRUE(!IsValidIp(L"169.254.1.1"), "Reject APIPA");
    ASSERT_TRUE(!IsValidIp(L""), "Reject empty");

    // Test 2: REG_MULTI_SZ Buffer Construction
    wchar_t multiSzBuf[64];
    unsigned long bytes = BuildMultiSz(L"192.168.180.3", multiSzBuf, 64);
    ASSERT_TRUE(bytes == (13 + 2) * sizeof(wchar_t), "Correct Multi-SZ Byte Length for IP");
    ASSERT_TRUE(multiSzBuf[13] == L'\0' && multiSzBuf[14] == L'\0', "Correct Double Null Termination for Multi-SZ");

    // Test 3: Combine DNS Servers
    wchar_t dnsBuf[64];
    CombineDns(L"8.8.8.8", L"8.8.4.4", dnsBuf, 64);
    ASSERT_TRUE(wcscmp(dnsBuf, L"8.8.8.8,8.8.4.4") == 0, "Combine DNS1 & DNS2 into comma-separated");

    CombineDns(L"8.8.8.8", L"", dnsBuf, 64);
    ASSERT_TRUE(wcscmp(dnsBuf, L"8.8.8.8") == 0, "Handle single DNS1");

    CombineDns(L"", L"1.1.1.1", dnsBuf, 64);
    ASSERT_TRUE(wcscmp(dnsBuf, L"1.1.1.1") == 0, "Handle single DNS2");

    // Test 4: Hostname Suffix Handling
    wchar_t hostBuf[64];
    FormatHostnameWithTm(L"PC-01", hostBuf, 64);
    ASSERT_TRUE(wcscmp(hostBuf, L"PC-01TM") == 0, "Append TM suffix to PC-01 -> PC-01TM");

    FormatHostnameWithTm(L"PC-01TM", hostBuf, 64);
    ASSERT_TRUE(wcscmp(hostBuf, L"PC-01TM") == 0, "Preserve existing TM suffix in PC-01TM");

    FormatHostnameWithTm(L"WARNET-15", hostBuf, 64);
    ASSERT_TRUE(wcscmp(hostBuf, L"WARNET-15TM") == 0, "Append TM suffix to custom host WARNET-15");

    printf("\n====================================================\n");
    printf(" Test Results: %d Passed, %d Failed\n", passed, failed);
    printf("====================================================\n");

    return failed > 0 ? 1 : 0;
}
