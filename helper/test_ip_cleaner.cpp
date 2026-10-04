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

// Format Clean Hostname (Menghilangkan suffix TM paksa, gunakan nama asli dari DHCP)
void FormatCleanHostname(const wchar_t* inHost, wchar_t* outHost, unsigned long maxChars) {
    if (!inHost || inHost[0] == L'\0') {
        wcscpy_s(outHost, maxChars, L"PC-CLIENT");
        return;
    }

    wcscpy_s(outHost, maxChars, inHost);
}

// Format Dual Registry Paths untuk penimpaan komprehensif
void FormatInterfacePath(const wchar_t* controlSet, const wchar_t* guidStr, wchar_t* outPath, unsigned long maxChars) {
    if (!controlSet || !guidStr || !outPath || maxChars == 0) return;
    swprintf_s(outPath, maxChars, L"\\Registry\\Machine\\System\\%s\\Services\\Tcpip\\Parameters\\Interfaces\\%s", controlSet, guidStr);
}

void FormatServiceTcpipPath(const wchar_t* controlSet, const wchar_t* guidStr, wchar_t* outPath, unsigned long maxChars) {
    if (!controlSet || !guidStr || !outPath || maxChars == 0) return;
    swprintf_s(outPath, maxChars, L"\\Registry\\Machine\\System\\%s\\Services\\%s\\Parameters\\Tcpip", controlSet, guidStr);
}

// Deteksi apakah buffer REG_MULTI_SZ memiliki lebih dari 1 entri string
int CountMultiSzStrings(const wchar_t* multiSz, unsigned long byteLen) {
    if (!multiSz || byteLen < 2 * sizeof(wchar_t)) return 0;
    unsigned long totalChars = byteLen / sizeof(wchar_t);
    int count = 0;
    unsigned long i = 0;
    while (i < totalChars && multiSz[i] != L'\0') {
        count++;
        while (i < totalChars && multiSz[i] != L'\0') {
            i++;
        }
        i++; // lewati null terminator
    }
    return count;
}

// Ekstraksi seluruh string dalam Multi-SZ menjadi satu string yang dipisahkan koma
void FormatMultiSzSummary(const wchar_t* multiSz, unsigned long byteLen, wchar_t* outBuf, unsigned long maxChars) {
    if (!multiSz || byteLen < sizeof(wchar_t) || !outBuf || maxChars == 0) {
        if (outBuf && maxChars > 0) outBuf[0] = L'\0';
        return;
    }
    outBuf[0] = L'\0';
    unsigned long charLen = byteLen / sizeof(wchar_t);
    unsigned long i = 0;
    bool first = true;

    while (i < charLen && multiSz[i] != L'\0') {
        const wchar_t* currentStr = &multiSz[i];
        if (!first) {
            wcscat_s(outBuf, maxChars, L", ");
        }
        wcscat_s(outBuf, maxChars, currentStr);
        first = false;

        while (i < charLen && multiSz[i] != L'\0') {
            i++;
        }
        i++; // lewati null terminator
    }
}

// Test Runner
int main() {
    printf("====================================================\n");
    printf(" Running Simple-Iscsi Boot Helper Unit Tests\n");
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
    ASSERT_TRUE(IsValidIp(L"10.10.10.25"), "Valid IP 10.10.10.25");
    ASSERT_TRUE(IsValidIp(L"255.255.255.0"), "Valid Mask 255.255.255.0");
    ASSERT_TRUE(IsValidIp(L"10.10.10.1"), "Valid Gateway 10.10.10.1");
    ASSERT_TRUE(!IsValidIp(L"0.0.0.0"), "Reject 0.0.0.0");
    ASSERT_TRUE(!IsValidIp(L"169.254.1.1"), "Reject APIPA");
    ASSERT_TRUE(!IsValidIp(L""), "Reject empty");

    // Test 2: REG_MULTI_SZ Buffer Construction (Single Entry)
    wchar_t multiSzBuf[64];
    unsigned long bytes = BuildMultiSz(L"10.10.10.25", multiSzBuf, 64);
    ASSERT_TRUE(bytes == (11 + 2) * sizeof(wchar_t), "Correct Multi-SZ Byte Length for IP");
    ASSERT_TRUE(multiSzBuf[11] == L'\0' && multiSzBuf[12] == L'\0', "Correct Double Null Termination for Multi-SZ");
    ASSERT_TRUE(CountMultiSzStrings(multiSzBuf, bytes) == 1, "Exactly 1 string entry in Multi-SZ (Prevent Double IP)");

    // Test 3: Multiple IP Detection in Old Stale Registry
    // Simulasi buffer kotor dari master image yang berisi 2 IP: "10.10.10.21\010.10.10.25\0\0"
    wchar_t dirtyMultiSz[64] = {
        L'1',L'0',L'.',L'1',L'0',L'.',L'1',L'0',L'.',L'2',L'1',L'\0',
        L'1',L'0',L'.',L'1',L'0',L'.',L'1',L'0',L'.',L'2',L'5',L'\0',
        L'\0'
    };
    unsigned long dirtyBytes = (12 + 12 + 1) * sizeof(wchar_t);
    ASSERT_TRUE(CountMultiSzStrings(dirtyMultiSz, dirtyBytes) == 2, "Detect 2 IP addresses in dirty buffer");

    // Lakukan overwrite bersih
    unsigned long cleanBytes = BuildMultiSz(L"10.10.10.25", dirtyMultiSz, 64);
    ASSERT_TRUE(CountMultiSzStrings(dirtyMultiSz, cleanBytes) == 1, "After overwrite: Exactly 1 IP address remaining");
    ASSERT_TRUE(wcscmp(dirtyMultiSz, L"10.10.10.25") == 0, "Overwritten IP matches new DHCP IP 10.10.10.25");

    // Test 4: Combine DNS Servers
    wchar_t dnsBuf[64];
    CombineDns(L"8.8.8.8", L"8.8.4.4", dnsBuf, 64);
    ASSERT_TRUE(wcscmp(dnsBuf, L"8.8.8.8,8.8.4.4") == 0, "Combine DNS1 & DNS2 into comma-separated");

    CombineDns(L"8.8.8.8", L"", dnsBuf, 64);
    ASSERT_TRUE(wcscmp(dnsBuf, L"8.8.8.8") == 0, "Handle single DNS1");

    CombineDns(L"", L"1.1.1.1", dnsBuf, 64);
    ASSERT_TRUE(wcscmp(dnsBuf, L"1.1.1.1") == 0, "Handle single DNS2");

    // Test 5: Clean Hostname from DHCP Option (No forced TM suffix)
    wchar_t hostBuf[64];
    FormatCleanHostname(L"PC-01", hostBuf, 64);
    ASSERT_TRUE(wcscmp(hostBuf, L"PC-01") == 0, "Preserve exact hostname PC-01 from DHCP");

    FormatCleanHostname(L"WARNET-15", hostBuf, 64);
    ASSERT_TRUE(wcscmp(hostBuf, L"WARNET-15") == 0, "Preserve exact hostname WARNET-15 from DHCP");

    FormatCleanHostname(L"", hostBuf, 64);
    ASSERT_TRUE(wcscmp(hostBuf, L"PC-CLIENT") == 0, "Fallback to PC-CLIENT if empty");

    // Test 6: Dual Registry Path Generation (Eliminasi Dual IP)
    const wchar_t sampleGuid[] = L"{35C00DC3-BC80-49F5-B537-6EC4E47F69C2}";
    wchar_t path1[256] = {0};
    wchar_t path2[256] = {0};
    FormatInterfacePath(L"CurrentControlSet", sampleGuid, path1, 256);
    FormatServiceTcpipPath(L"CurrentControlSet", sampleGuid, path2, 256);

    ASSERT_TRUE(wcscmp(path1, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\{35C00DC3-BC80-49F5-B537-6EC4E47F69C2}") == 0, "Format CurrentControlSet Interfaces Path");
    ASSERT_TRUE(wcscmp(path2, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\{35C00DC3-BC80-49F5-B537-6EC4E47F69C2}\\Parameters\\Tcpip") == 0, "Format CurrentControlSet Services Tcpip Path");

    wchar_t cs001_path1[256] = {0};
    wchar_t cs001_path2[256] = {0};
    FormatInterfacePath(L"ControlSet001", sampleGuid, cs001_path1, 256);
    FormatServiceTcpipPath(L"ControlSet001", sampleGuid, cs001_path2, 256);

    ASSERT_TRUE(wcscmp(cs001_path1, L"\\Registry\\Machine\\System\\ControlSet001\\Services\\Tcpip\\Parameters\\Interfaces\\{35C00DC3-BC80-49F5-B537-6EC4E47F69C2}") == 0, "Format ControlSet001 Interfaces Path");
    ASSERT_TRUE(wcscmp(cs001_path2, L"\\Registry\\Machine\\System\\ControlSet001\\Services\\{35C00DC3-BC80-49F5-B537-6EC4E47F69C2}\\Parameters\\Tcpip") == 0, "Format ControlSet001 Services Tcpip Path");

    // Test 7: Multi-SZ String Extraction and Formatting
    wchar_t doubleIpMultiSz[64] = {
        L'1',L'9',L'2',L'.',L'1',L'6',L'8',L'.',L'1',L'8',L'0',L'.',L'1',L'0',L'\0',
        L'1',L'9',L'2',L'.',L'1',L'6',L'8',L'.',L'1',L'8',L'0',L'.',L'2',L'\0',
        L'\0'
    };
    unsigned long doubleBytes = (15 + 14 + 1) * sizeof(wchar_t);
    wchar_t summaryBuf[128] = {0};
    FormatMultiSzSummary(doubleIpMultiSz, doubleBytes, summaryBuf, 128);
    ASSERT_TRUE(wcscmp(summaryBuf, L"192.168.180.10, 192.168.180.2") == 0, "Extract and format multiple IPs from dirty buffer");

    wchar_t singleSummary[128] = {0};
    FormatMultiSzSummary(multiSzBuf, bytes, singleSummary, 128);
    ASSERT_TRUE(wcscmp(singleSummary, L"10.10.10.25") == 0, "Extract single IP from clean buffer");

    printf("\n====================================================\n");
    printf(" Test Results: %d Passed, %d Failed\n", passed, failed);
    printf("====================================================\n");

    return failed > 0 ? 1 : 0;
}

