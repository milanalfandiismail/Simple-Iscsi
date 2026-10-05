typedef long NTSTATUS;

#define NT_SUCCESS(Status) (((NTSTATUS)(Status)) >= 0)
#define STATUS_SUCCESS ((NTSTATUS)0x00000000L)
#define STATUS_NO_MORE_ENTRIES ((NTSTATUS)0x8000001AL)

#define NTSYSAPI __declspec(dllimport)
#define NTAPI __stdcall

#ifdef _WIN64
typedef unsigned __int64 size_t;
#else
typedef unsigned int size_t;
#endif

// Prototipe & implementasi memcpy / memset tanpa CRT
extern "C" void* __cdecl memset(void* dest, int c, size_t count);
extern "C" void* __cdecl memcpy(void* dest, const void* src, size_t count);

#pragma function(memset)
#pragma function(memcpy)

extern "C" void* __cdecl memset(void* dest, int c, size_t count) {
    unsigned char* p = (unsigned char*)dest;
    while (count--) {
        *p++ = (unsigned char)c;
    }
    return dest;
}

extern "C" void* __cdecl memcpy(void* dest, const void* src, size_t count) {
    unsigned char* d = (unsigned char*)dest;
    const unsigned char* s = (const unsigned char*)src;
    while (count--) {
        *d++ = *s++;
    }
    return dest;
}

extern "C" void __cdecl _chkstk() {}
extern "C" void __cdecl __chkstk() {}

typedef union _LARGE_INTEGER {
    struct {
        unsigned long LowPart;
        long HighPart;
    } u;
    long long QuadPart;
} LARGE_INTEGER;

typedef struct _UNICODE_STRING {
    unsigned short Length;
    unsigned short MaximumLength;
    wchar_t* Buffer;
} UNICODE_STRING, *PUNICODE_STRING;

typedef struct _OBJECT_ATTRIBUTES {
    unsigned long Length;
    void* RootDirectory;
    PUNICODE_STRING ObjectName;
    unsigned long Attributes;
    void* SecurityDescriptor;
    void* SecurityQualityOfService;
} OBJECT_ATTRIBUTES, *POBJECT_ATTRIBUTES;

#define OBJ_CASE_INSENSITIVE 0x00000040L

typedef enum _KEY_INFORMATION_CLASS {
    KeyBasicInformation,
    KeyNodeInformation,
    KeyFullInformation,
    KeyNameInformation,
    KeyCachedInformation,
    KeyFlagsInformation,
    KeyVirtualizationInformation,
    KeyHandleTagsInformation,
    MaxKeyInfoClass
} KEY_INFORMATION_CLASS;

typedef struct _KEY_BASIC_INFORMATION {
    LARGE_INTEGER LastWriteTime;
    unsigned long TitleIndex;
    unsigned long NameLength;
    wchar_t Name[1]; // Variable size
} KEY_BASIC_INFORMATION, *PKEY_BASIC_INFORMATION;

typedef enum _KEY_VALUE_INFORMATION_CLASS {
    KeyValueBasicInformation,
    KeyValueFullInformation,
    KeyValuePartialInformation,
    KeyValueFullInformationAlign64,
    KeyValuePartialInformationAlign64
} KEY_VALUE_INFORMATION_CLASS;

typedef struct _KEY_VALUE_PARTIAL_INFORMATION {
    unsigned long TitleIndex;
    unsigned long Type;
    unsigned long DataLength;
    unsigned char Data[1]; // Variable size
} KEY_VALUE_PARTIAL_INFORMATION, *PKEY_VALUE_PARTIAL_INFORMATION;

typedef struct _IO_STATUS_BLOCK {
    union {
        NTSTATUS Status;
        void* Pointer;
    };
    unsigned long long Information;
} IO_STATUS_BLOCK, *PIO_STATUS_BLOCK;

#define FILE_OVERWRITE_IF              0x00000005
#define FILE_SYNCHRONOUS_IO_NONALERT   0x00000020
#define FILE_NON_DIRECTORY_FILE        0x00000040
#define GENERIC_WRITE                  0x40000000L
#define FILE_GENERIC_WRITE             (GENERIC_WRITE | 0x00100000L | 0x0002 | 0x0004 | 0x0010 | 0x0040)
#define SYNCHRONIZE                    0x00100000L

#define REG_SZ 1
#define REG_BINARY 3
#define REG_DWORD 4
#define REG_MULTI_SZ 7

#define SystemFirmwareTableInformation 76

typedef struct _SYSTEM_FIRMWARE_TABLE_INFORMATION {
    unsigned long ProviderSignature;
    unsigned long Action;
    unsigned long TableID;
    unsigned long TableBufferLength;
    unsigned char TableBuffer[1];
} SYSTEM_FIRMWARE_TABLE_INFORMATION, *PSYSTEM_FIRMWARE_TABLE_INFORMATION;

extern "C" {
    NTSYSAPI void NTAPI RtlInitUnicodeString(
        PUNICODE_STRING DestinationString,
        const wchar_t* SourceString
    );

    NTSYSAPI NTSTATUS NTAPI NtOpenKey(
        void** KeyHandle,
        unsigned long DesiredAccess,
        POBJECT_ATTRIBUTES ObjectAttributes
    );

    NTSYSAPI NTSTATUS NTAPI NtCreateKey(
        void** KeyHandle,
        unsigned long DesiredAccess,
        POBJECT_ATTRIBUTES ObjectAttributes,
        unsigned long TitleIndex,
        PUNICODE_STRING Class,
        unsigned long CreateOptions,
        unsigned long* Disposition
    );

    NTSYSAPI NTSTATUS NTAPI NtEnumerateKey(
        void* KeyHandle,
        unsigned long Index,
        KEY_INFORMATION_CLASS KeyInformationClass,
        void* KeyInformation,
        unsigned long Length,
        unsigned long* ResultLength
    );

    NTSYSAPI NTSTATUS NTAPI NtQueryValueKey(
        void* KeyHandle,
        PUNICODE_STRING ValueName,
        KEY_VALUE_INFORMATION_CLASS KeyValueInformationClass,
        void* KeyValueInformation,
        unsigned long Length,
        unsigned long* ResultLength
    );

    NTSYSAPI NTSTATUS NTAPI NtSetValueKey(
        void* KeyHandle,
        PUNICODE_STRING ValueName,
        unsigned long TitleIndex,
        unsigned long Type,
        const void* Data,
        unsigned long DataSize
    );

    NTSYSAPI NTSTATUS NTAPI NtCreateFile(
        void** FileHandle,
        unsigned long DesiredAccess,
        POBJECT_ATTRIBUTES ObjectAttributes,
        PIO_STATUS_BLOCK IoStatusBlock,
        LARGE_INTEGER* AllocationSize,
        unsigned long FileAttributes,
        unsigned long ShareAccess,
        unsigned long CreateDisposition,
        unsigned long CreateOptions,
        void* EaBuffer,
        unsigned long EaLength
    );

    NTSYSAPI NTSTATUS NTAPI NtWriteFile(
        void* FileHandle,
        void* Event,
        void* ApcRoutine,
        void* ApcContext,
        PIO_STATUS_BLOCK IoStatusBlock,
        const void* Buffer,
        unsigned long Length,
        LARGE_INTEGER* ByteOffset,
        unsigned long* Key
    );

    NTSYSAPI NTSTATUS NTAPI NtQuerySystemInformation(
        unsigned long SystemInformationClass,
        void* SystemInformation,
        unsigned long SystemInformationLength,
        unsigned long* ReturnLength
    );

    NTSYSAPI NTSTATUS NTAPI NtDeleteValueKey(
        void* KeyHandle,
        PUNICODE_STRING ValueName
    );

    NTSYSAPI NTSTATUS NTAPI NtClose(
        void* Handle
    );

    NTSYSAPI NTSTATUS NTAPI NtTerminateProcess(
        void* ProcessHandle,
        NTSTATUS ExitStatus
    );
}

#define KEY_QUERY_VALUE 0x0001
#define KEY_SET_VALUE   0x0002
#define KEY_ENUMERATE_SUB_KEYS 0x0008

// -------------------------------------------------------------
// Native File Logger (Mencatat ke C:\Windows\helper.log)
// -------------------------------------------------------------

void* g_hLogFile = nullptr;

void LogOpen() {
    UNICODE_STRING logPath;
    RtlInitUnicodeString(&logPath, L"\\SystemRoot\\helper.log");

    OBJECT_ATTRIBUTES objAttr;
    objAttr.Length = sizeof(OBJECT_ATTRIBUTES);
    objAttr.RootDirectory = nullptr;
    objAttr.ObjectName = &logPath;
    objAttr.Attributes = OBJ_CASE_INSENSITIVE;
    objAttr.SecurityDescriptor = nullptr;
    objAttr.SecurityQualityOfService = nullptr;

    IO_STATUS_BLOCK ioStatus;
    NtCreateFile(
        &g_hLogFile,
        FILE_GENERIC_WRITE | SYNCHRONIZE,
        &objAttr,
        &ioStatus,
        nullptr,
        0x00000080, // FILE_ATTRIBUTE_NORMAL
        0x00000001 | 0x00000002, // FILE_SHARE_READ | FILE_SHARE_WRITE
        FILE_OVERWRITE_IF,
        FILE_SYNCHRONOUS_IO_NONALERT | FILE_NON_DIRECTORY_FILE,
        nullptr,
        0
    );
}

void LogWriteA(const char* str) {
    if (!g_hLogFile || !str) return;
    unsigned long len = 0;
    while (str[len] != '\0') len++;
    if (len == 0) return;

    IO_STATUS_BLOCK ioStatus;
    NtWriteFile(g_hLogFile, nullptr, nullptr, nullptr, &ioStatus, str, len, nullptr, nullptr);
}

void LogWriteW(const wchar_t* wstr) {
    if (!g_hLogFile || !wstr) return;
    char buffer[256];
    unsigned long i = 0;
    while (wstr[i] != L'\0' && i < 255) {
        wchar_t wc = wstr[i];
        buffer[i] = (wc < 128) ? (char)wc : '?';
        i++;
    }
    buffer[i] = '\0';
    LogWriteA(buffer);
}

void LogClose() {
    if (g_hLogFile) {
        NtClose(g_hLogFile);
        g_hLogFile = nullptr;
    }
}

// -------------------------------------------------------------
// Helper String & Network Utilities (No CRT, Safe Bounds)
// -------------------------------------------------------------

bool StrEqual(const wchar_t* a, const wchar_t* b) {
    if (!a || !b) return false;
    while (*a && *b) {
        if (*a != *b) return false;
        a++;
        b++;
    }
    return *a == *b;
}

bool StrEqualN(const wchar_t* a, const wchar_t* b, unsigned long nChars) {
    if (!a || !b) return false;
    for (unsigned long i = 0; i < nChars; i++) {
        if (a[i] == L'\0' && b[i] == L'\0') return true;
        if (a[i] != b[i]) return false;
        if (a[i] == L'\0' || b[i] == L'\0') return false;
    }
    return (b[nChars] == L'\0');
}

bool StrStartsWith(const wchar_t* str, const wchar_t* prefix) {
    if (!str || !prefix) return false;
    while (*prefix) {
        if (*str != *prefix) return false;
        str++;
        prefix++;
    }
    return true;
}

unsigned long StrLen(const wchar_t* str) {
    if (!str) return 0;
    unsigned long len = 0;
    while (str[len] != L'\0') len++;
    return len;
}

void StrCopy(wchar_t* dest, const wchar_t* src, unsigned long maxChars) {
    if (!dest || !src || maxChars == 0) return;
    unsigned long i = 0;
    while (src[i] != L'\0' && i + 1 < maxChars) {
        dest[i] = src[i];
        i++;
    }
    dest[i] = L'\0';
}

void StrCat(wchar_t* dest, const wchar_t* src, unsigned long maxChars) {
    if (!dest || !src || maxChars == 0) return;
    unsigned long dLen = StrLen(dest);
    unsigned long i = 0;
    while (src[i] != L'\0' && (dLen + i + 1) < maxChars) {
        dest[dLen + i] = src[i];
        i++;
    }
    dest[dLen + i] = L'\0';
}

void UintToWstr(unsigned int val, wchar_t* outBuf, unsigned long maxChars) {
    if (!outBuf || maxChars < 2) return;
    if (val == 0) {
        outBuf[0] = L'0';
        outBuf[1] = L'\0';
        return;
    }

    wchar_t temp[16];
    int tIdx = 0;
    while (val > 0 && tIdx < 15) {
        temp[tIdx++] = L'0' + (val % 10);
        val /= 10;
    }

    unsigned long outIdx = 0;
    while (tIdx > 0 && outIdx + 1 < maxChars) {
        outBuf[outIdx++] = temp[--tIdx];
    }
    outBuf[outIdx] = L'\0';
}

bool IsValidIp(const wchar_t* ipStr) {
    if (!ipStr || ipStr[0] == L'\0') return false;
    if (StrEqual(ipStr, L"0.0.0.0")) return false;
    if (StrStartsWith(ipStr, L"169.254.")) return false;

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

// Konversi 16-byte iBFT IPv4 / IPv6 ke string IP (e.g. "192.168.180.4")
bool FormatIpv4FromBytes(const unsigned char* raw16, wchar_t* outStr, unsigned long maxChars) {
    if (!raw16 || !outStr || maxChars < 16) return false;
    outStr[0] = L'\0';

    // 1. Coba byte 12..15 (IPv4-mapped IPv6: 0.0.0.0.0.0.0.0.0.0.0.0.A.B.C.D)
    if (raw16[12] != 0 && raw16[12] != 127) {
        wchar_t seg[8];
        UintToWstr(raw16[12], seg, 8); StrCopy(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
        UintToWstr(raw16[13], seg, 8); StrCat(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
        UintToWstr(raw16[14], seg, 8); StrCat(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
        UintToWstr(raw16[15], seg, 8); StrCat(outStr, seg, maxChars);
        if (IsValidIp(outStr)) return true;
    }

    // 2. Coba byte 0..3 (Direct IPv4)
    if (raw16[0] != 0 && raw16[0] != 127) {
        wchar_t seg[8];
        UintToWstr(raw16[0], seg, 8); StrCopy(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
        UintToWstr(raw16[1], seg, 8); StrCat(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
        UintToWstr(raw16[2], seg, 8); StrCat(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
        UintToWstr(raw16[3], seg, 8); StrCat(outStr, seg, maxChars);
        if (IsValidIp(outStr)) return true;
    }

    return false;
}

// Konversi CIDR prefix length (misal 24) ke Subnet Mask ("255.255.255.0")
void PrefixToSubnetMask(unsigned char prefix, wchar_t* outStr, unsigned long maxChars) {
    if (prefix == 0 || prefix > 32) {
        StrCopy(outStr, L"255.255.255.0", maxChars);
        return;
    }

    unsigned long mask = 0;
    if (prefix > 0) {
        mask = (0xFFFFFFFFUL << (32 - prefix)) & 0xFFFFFFFFUL;
    }

    unsigned char b0 = (unsigned char)((mask >> 24) & 0xFF);
    unsigned char b1 = (unsigned char)((mask >> 16) & 0xFF);
    unsigned char b2 = (unsigned char)((mask >> 8) & 0xFF);
    unsigned char b3 = (unsigned char)(mask & 0xFF);

    wchar_t seg[8];
    UintToWstr(b0, seg, 8); StrCopy(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
    UintToWstr(b1, seg, 8); StrCat(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
    UintToWstr(b2, seg, 8); StrCat(outStr, seg, maxChars); StrCat(outStr, L".", maxChars);
    UintToWstr(b3, seg, 8); StrCat(outStr, seg, maxChars);
}

// Format string ke REG_MULTI_SZ (diakhiri double null: "192.168.180.1\0\0")
unsigned long BuildMultiSz(const wchar_t* src, wchar_t* outBuf, unsigned long maxChars) {
    if (!src || maxChars < 3) return 0;
    unsigned long len = StrLen(src);
    if (len + 2 > maxChars) return 0;

    for (unsigned long i = 0; i < len; i++) {
        outBuf[i] = src[i];
    }
    outBuf[len] = L'\0';
    outBuf[len + 1] = L'\0';

    return (len + 2) * sizeof(wchar_t);
}

// Membaca string REG_SZ atau REG_MULTI_SZ dari registry
bool ReadRegMultiSz(void* hKey, const wchar_t* valNameStr, wchar_t* outBuf, unsigned long maxChars) {
    if (!hKey || !valNameStr || !outBuf || maxChars == 0) return false;
    outBuf[0] = L'\0';

    UNICODE_STRING valName;
    RtlInitUnicodeString(&valName, valNameStr);

    unsigned char queryBuf[512] = {0};
    unsigned long qLen = 0;
    if (NT_SUCCESS(NtQueryValueKey(hKey, &valName, KeyValuePartialInformation, queryBuf, sizeof(queryBuf), &qLen))) {
        PKEY_VALUE_PARTIAL_INFORMATION pValInfo = (PKEY_VALUE_PARTIAL_INFORMATION)queryBuf;
        if (pValInfo->DataLength > 0) {
            if (pValInfo->Type == REG_SZ) {
                wchar_t* rawStr = (wchar_t*)pValInfo->Data;
                unsigned long rawLenChars = pValInfo->DataLength / sizeof(wchar_t);
                if (rawLenChars > 0 && rawStr[rawLenChars - 1] == L'\0') rawLenChars--;
                unsigned long copyLen = rawLenChars < (maxChars - 1) ? rawLenChars : (maxChars - 1);
                for (unsigned long k = 0; k < copyLen; k++) outBuf[k] = rawStr[k];
                outBuf[copyLen] = L'\0';
                return true;
            } else if (pValInfo->Type == REG_MULTI_SZ) {
                const wchar_t* rawMulti = (const wchar_t*)pValInfo->Data;
                unsigned long charLen = pValInfo->DataLength / sizeof(wchar_t);
                unsigned long i = 0;
                unsigned long outIdx = 0;
                while (i < charLen && rawMulti[i] != L'\0' && outIdx < (maxChars - 1)) {
                    outBuf[outIdx++] = rawMulti[i++];
                }
                outBuf[outIdx] = L'\0';
                return true;
            }
        }
    }
    return false;
}

// Format Clean Hostname sesuai konfigurasi asli DHCP / iBFT
void FormatCleanHostname(const wchar_t* inHost, wchar_t* outHost, unsigned long maxChars) {
    if (!inHost || inHost[0] == L'\0') {
        StrCopy(outHost, L"PC-CLIENT", maxChars);
        return;
    }
    StrCopy(outHost, inHost, maxChars);
}

void LogWriteHex(unsigned long val) {
    char hexBuf[16];
    const char hexChars[] = "0123456789ABCDEF";
    hexBuf[0] = '0';
    hexBuf[1] = 'x';
    for (int i = 7; i >= 0; i--) {
        hexBuf[2 + (7 - i)] = hexChars[(val >> (i * 4)) & 0xF];
    }
    hexBuf[10] = '\0';
    LogWriteA(hexBuf);
}

// Menulis marker SimpleIscsiBoot untuk disinkronkan oleh helper-svc di user-mode
void WriteSimpleIscsiMarker(
    const wchar_t* targetIp,
    const wchar_t* gwIp,
    const wchar_t* mask,
    const wchar_t* dns1,
    const wchar_t* dns2,
    const wchar_t* combinedDns,
    const wchar_t* host
) {
    UNICODE_STRING keyPath;
    RtlInitUnicodeString(&keyPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\SimpleIscsiBoot");
    OBJECT_ATTRIBUTES objAttr;
    objAttr.Length = sizeof(OBJECT_ATTRIBUTES);
    objAttr.RootDirectory = nullptr;
    objAttr.ObjectName = &keyPath;
    objAttr.Attributes = OBJ_CASE_INSENSITIVE;
    objAttr.SecurityDescriptor = nullptr;
    objAttr.SecurityQualityOfService = nullptr;

    void* hKey = nullptr;
    unsigned long disposition = 0;
    NTSTATUS status = NtCreateKey(&hKey, 0xF003F, &objAttr, 0, nullptr, 0, &disposition);
    if (!NT_SUCCESS(status)) {
        status = NtOpenKey(&hKey, KEY_SET_VALUE, &objAttr);
    }

    if (NT_SUCCESS(status)) {
        UNICODE_STRING valTargetIp, valGw, valMask, valDns1, valDns2, valNameServer, valHost, valPurge;
        RtlInitUnicodeString(&valTargetIp, L"TargetIp");
        RtlInitUnicodeString(&valGw, L"GatewayIp");
        RtlInitUnicodeString(&valMask, L"SubnetMask");
        RtlInitUnicodeString(&valDns1, L"Dns1");
        RtlInitUnicodeString(&valDns2, L"Dns2");
        RtlInitUnicodeString(&valNameServer, L"NameServer");
        RtlInitUnicodeString(&valHost, L"Hostname");
        RtlInitUnicodeString(&valPurge, L"PurgeNeeded");

        unsigned long ipByteLen = (StrLen(targetIp) + 1) * sizeof(wchar_t);
        unsigned long gwByteLen = (StrLen(gwIp) + 1) * sizeof(wchar_t);
        unsigned long maskByteLen = (StrLen(mask) + 1) * sizeof(wchar_t);
        unsigned long dns1ByteLen = (StrLen(dns1) + 1) * sizeof(wchar_t);
        unsigned long dns2ByteLen = (StrLen(dns2) + 1) * sizeof(wchar_t);
        unsigned long nsByteLen = (StrLen(combinedDns) + 1) * sizeof(wchar_t);
        unsigned long hostByteLen = (StrLen(host) + 1) * sizeof(wchar_t);
        unsigned long purgeVal = 1;

        NtSetValueKey(hKey, &valTargetIp, 0, REG_SZ, (void*)targetIp, ipByteLen);
        NtSetValueKey(hKey, &valGw, 0, REG_SZ, (void*)gwIp, gwByteLen);
        NtSetValueKey(hKey, &valMask, 0, REG_SZ, (void*)mask, maskByteLen);
        NtSetValueKey(hKey, &valDns1, 0, REG_SZ, (void*)dns1, dns1ByteLen);
        NtSetValueKey(hKey, &valDns2, 0, REG_SZ, (void*)dns2, dns2ByteLen);
        NtSetValueKey(hKey, &valNameServer, 0, REG_SZ, (void*)combinedDns, nsByteLen);
        NtSetValueKey(hKey, &valHost, 0, REG_SZ, (void*)host, hostByteLen);
        NtSetValueKey(hKey, &valPurge, 0, REG_DWORD, &purgeVal, sizeof(purgeVal));

        NtClose(hKey);
        LogWriteA("[+] SimpleIscsiBoot Marker Created Successfully in Registry.\r\n");
    }
}

// Menyelaraskan DNS dan membersihkan DefaultGateway 0.0.0.0 di Registry tanpa merusak DHCP
void SanitizeDnsAndGateway(const wchar_t* gwIp, const wchar_t* combinedDns) {
    if (!IsValidIp(gwIp) && (!combinedDns || combinedDns[0] == L'\0')) return;

    LogWriteA("[+] Sanitizing DNS and DefaultGateway in Registry...\r\n");

    wchar_t multiSzGateway[64] = {0};
    unsigned long gwByteLen = 0;
    if (IsValidIp(gwIp)) {
        gwByteLen = BuildMultiSz(gwIp, multiSzGateway, 64);
    }

    const wchar_t cleanMetric[] = L"0\0\0";
    unsigned long metricByteLen = sizeof(cleanMetric);
    unsigned long dnsByteLen = (StrLen(combinedDns) + 1) * sizeof(wchar_t);

    // 1. Tulis global NameServer di Tcpip\Parameters
    if (dnsByteLen > sizeof(wchar_t)) {
        UNICODE_STRING tcpipParamsPath;
        RtlInitUnicodeString(&tcpipParamsPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\Tcpip\\Parameters");
        OBJECT_ATTRIBUTES tcpipAttr;
        tcpipAttr.Length = sizeof(OBJECT_ATTRIBUTES);
        tcpipAttr.RootDirectory = nullptr;
        tcpipAttr.ObjectName = &tcpipParamsPath;
        tcpipAttr.Attributes = OBJ_CASE_INSENSITIVE;
        tcpipAttr.SecurityDescriptor = nullptr;
        tcpipAttr.SecurityQualityOfService = nullptr;

        void* hTcpipParams = nullptr;
        if (NT_SUCCESS(NtOpenKey(&hTcpipParams, KEY_SET_VALUE, &tcpipAttr))) {
            UNICODE_STRING valNs;
            RtlInitUnicodeString(&valNs, L"NameServer");
            NtSetValueKey(hTcpipParams, &valNs, 0, REG_SZ, (void*)combinedDns, dnsByteLen);

            UNICODE_STRING valDhcpNs;
            RtlInitUnicodeString(&valDhcpNs, L"DhcpNameServer");
            NtSetValueKey(hTcpipParams, &valDhcpNs, 0, REG_SZ, (void*)combinedDns, dnsByteLen);

            NtClose(hTcpipParams);
        }
    }

    // 2. Bersihkan 0.0.0.0 Gateway dan tulis NameServer di setiap interface
    UNICODE_STRING interfacesPath;
    RtlInitUnicodeString(&interfacesPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces");

    OBJECT_ATTRIBUTES objAttr;
    objAttr.Length = sizeof(OBJECT_ATTRIBUTES);
    objAttr.RootDirectory = nullptr;
    objAttr.ObjectName = &interfacesPath;
    objAttr.Attributes = OBJ_CASE_INSENSITIVE;
    objAttr.SecurityDescriptor = nullptr;
    objAttr.SecurityQualityOfService = nullptr;

    void* hInterfacesKey = nullptr;
    if (NT_SUCCESS(NtOpenKey(&hInterfacesKey, KEY_ENUMERATE_SUB_KEYS | KEY_QUERY_VALUE, &objAttr))) {
        unsigned char enumBuffer[512];
        unsigned long resultLength = 0;

        for (unsigned long index = 0; ; index++) {
            NTSTATUS status = NtEnumerateKey(hInterfacesKey, index, KeyBasicInformation, enumBuffer, sizeof(enumBuffer), &resultLength);
            if (!NT_SUCCESS(status)) break;

            PKEY_BASIC_INFORMATION pKeyInfo = (PKEY_BASIC_INFORMATION)enumBuffer;
            unsigned long nameLenChars = pKeyInfo->NameLength / sizeof(wchar_t);
            wchar_t safeGuid[128] = {0};
            unsigned long cpyLen = nameLenChars < 127 ? nameLenChars : 127;
            for (unsigned long i = 0; i < cpyLen; i++) safeGuid[i] = pKeyInfo->Name[i];
            safeGuid[cpyLen] = L'\0';

            wchar_t fullPath[256];
            StrCopy(fullPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\", 256);
            StrCat(fullPath, safeGuid, 256);

            UNICODE_STRING subKeyName;
            RtlInitUnicodeString(&subKeyName, fullPath);
            OBJECT_ATTRIBUTES subObjAttr;
            subObjAttr.Length = sizeof(OBJECT_ATTRIBUTES);
            subObjAttr.RootDirectory = nullptr;
            subObjAttr.ObjectName = &subKeyName;
            subObjAttr.Attributes = OBJ_CASE_INSENSITIVE;
            subObjAttr.SecurityDescriptor = nullptr;
            subObjAttr.SecurityQualityOfService = nullptr;

            void* hSubKey = nullptr;
            if (NT_SUCCESS(NtOpenKey(&hSubKey, KEY_SET_VALUE | KEY_QUERY_VALUE, &subObjAttr))) {
                // Tulis NameServer pada interface
                if (dnsByteLen > sizeof(wchar_t)) {
                    UNICODE_STRING valDns;
                    RtlInitUnicodeString(&valDns, L"NameServer");
                    NtSetValueKey(hSubKey, &valDns, 0, REG_SZ, (void*)combinedDns, dnsByteLen);

                    UNICODE_STRING valDhcpDns;
                    RtlInitUnicodeString(&valDhcpDns, L"DhcpNameServer");
                    NtSetValueKey(hSubKey, &valDhcpDns, 0, REG_SZ, (void*)combinedDns, dnsByteLen);
                }

                // Periksa DefaultGateway lama
                wchar_t existingGw[128] = {0};
                ReadRegMultiSz(hSubKey, L"DefaultGateway", existingGw, 128);

                // Jika DefaultGateway kosong atau berisi 0.0.0.0, perbaiki dengan gateway valid
                if (gwByteLen > 0 && (existingGw[0] == L'\0' || StrEqual(existingGw, L"0.0.0.0") || StrStartsWith(existingGw, L"0.0.0.0"))) {
                    UNICODE_STRING valGw, valMetric;
                    RtlInitUnicodeString(&valGw, L"DefaultGateway");
                    NtSetValueKey(hSubKey, &valGw, 0, REG_MULTI_SZ, (void*)multiSzGateway, gwByteLen);

                    RtlInitUnicodeString(&valMetric, L"DefaultGatewayMetric");
                    NtSetValueKey(hSubKey, &valMetric, 0, REG_MULTI_SZ, (void*)cleanMetric, metricByteLen);

                    LogWriteA("    [+] Fixed DefaultGateway on interface: "); LogWriteW(safeGuid); LogWriteA("\r\n");
                }

                NtClose(hSubKey);
            }
        }
        NtClose(hInterfacesKey);
    }
}

// Menulis parameter optimasi booting cepat ke Services\iScsiPrt\Parameters
void ConfigureIscsiPrtParameters() {
    UNICODE_STRING keyPath;
    RtlInitUnicodeString(&keyPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\iScsiPrt\\Parameters");
    OBJECT_ATTRIBUTES objAttr;
    objAttr.Length = sizeof(OBJECT_ATTRIBUTES);
    objAttr.RootDirectory = nullptr;
    objAttr.ObjectName = &keyPath;
    objAttr.Attributes = OBJ_CASE_INSENSITIVE;
    objAttr.SecurityDescriptor = nullptr;
    objAttr.SecurityQualityOfService = nullptr;

    void* hKey = nullptr;
    unsigned long disposition = 0;
    NTSTATUS status = NtCreateKey(&hKey, KEY_SET_VALUE, &objAttr, 0, nullptr, 0, &disposition);
    if (!NT_SUCCESS(status)) {
        status = NtOpenKey(&hKey, KEY_SET_VALUE, &objAttr);
    }

    if (NT_SUCCESS(status)) {
        UNICODE_STRING valWait, valDelay, valLink, valHold;
        RtlInitUnicodeString(&valWait, L"WaitForNetworkAtBoot");
        RtlInitUnicodeString(&valDelay, L"DelayForNetworkAtBoot");
        RtlInitUnicodeString(&valLink, L"LinkDownTime");
        RtlInitUnicodeString(&valHold, L"MaxRequestHoldTime");

        unsigned long waitVal = 1;
        unsigned long delayVal = 5; // 5 detik max delay tunggu link PHY tanpa stall lama
        unsigned long linkVal = 60;
        unsigned long holdVal = 60;

        NtSetValueKey(hKey, &valWait, 0, REG_DWORD, &waitVal, sizeof(waitVal));
        NtSetValueKey(hKey, &valDelay, 0, REG_DWORD, &delayVal, sizeof(delayVal));
        NtSetValueKey(hKey, &valLink, 0, REG_DWORD, &linkVal, sizeof(linkVal));
        NtSetValueKey(hKey, &valHold, 0, REG_DWORD, &holdVal, sizeof(holdVal));

        NtClose(hKey);
        LogWriteA("[+] Configured iScsiPrt Fast-Boot Parameters (WaitForNetworkAtBoot=1, DelayForNetworkAtBoot=5).\r\n");
    }
}

// -------------------------------------------------------------
// iBFT Parser Structure (RFC 4173 Standard)
// -------------------------------------------------------------

#pragma pack(push, 1)
typedef struct _IBFT_STRUCTURE_HEADER {
    unsigned char StructureId; // 1 = Control, 2 = Initiator, 3 = NIC, 4 = Target
    unsigned char Version;
    unsigned short Length;
    unsigned char Index;
    unsigned char Flags;
} IBFT_STRUCTURE_HEADER;

typedef struct _IBFT_CONTROL {
    IBFT_STRUCTURE_HEADER Header;
    unsigned short InitiatorOffset;
    unsigned short Nic0Offset;
    unsigned short Target0Offset;
    unsigned short Nic1Offset;
    unsigned short Target1Offset;
} IBFT_CONTROL;

typedef struct _IBFT_INITIATOR {
    IBFT_STRUCTURE_HEADER Header;
    unsigned char IsnsServer[16];
    unsigned char SlpServer[16];
    unsigned char PrimaryRadiusServer[16];
    unsigned char SecondaryRadiusServer[16];
    unsigned short InitiatorNameLength;
    unsigned short InitiatorNameOffset;
} IBFT_INITIATOR;

typedef struct _IBFT_NIC {
    IBFT_STRUCTURE_HEADER Header;
    unsigned char IpAddress[16];
    unsigned char SubnetMaskPrefix;
    unsigned char Origin;
    unsigned char Gateway[16];
    unsigned char PrimaryDns[16];
    unsigned char SecondaryDns[16];
    unsigned char DhcpServer[16];
    unsigned short Vlan;
    unsigned char MacAddress[6];
    unsigned short PciBusDevFunc;
    unsigned short HostNameLength;
    unsigned short HostNameOffset;
} IBFT_NIC;

typedef struct _IBFT_TARGET {
    IBFT_STRUCTURE_HEADER Header;
    unsigned char TargetIpAddress[16];
    unsigned short TargetPort;
    unsigned char BootLun[8];
    unsigned char ChapType;
    unsigned char NicAssociation;
    unsigned short TargetNameLength;
    unsigned short TargetNameOffset;
} IBFT_TARGET;
#pragma pack(pop)

#define MAKE_FOURCC(a, b, c, d) \
    (((unsigned long)(unsigned char)(a)) | \
    (((unsigned long)(unsigned char)(b)) << 8) | \
    (((unsigned long)(unsigned char)(c)) << 16) | \
    (((unsigned long)(unsigned char)(d)) << 24))

// Membaca dan mem-parse tabel iBFT dari ACPI Firmware secara aman dengan bound-checking ketat
bool ReadParametersFromIBFT(wchar_t* outHost, wchar_t* outIp, wchar_t* outMask, wchar_t* outGw, wchar_t* outDns1, wchar_t* outDns2) {
    LogWriteA("[+] Querying ACPI iBFT Firmware Table via NtQuerySystemInformation...\r\n");

    static unsigned char queryBuffer[4096];
    PSYSTEM_FIRMWARE_TABLE_INFORMATION pFirmware = (PSYSTEM_FIRMWARE_TABLE_INFORMATION)queryBuffer;
    const unsigned long maxPayload = sizeof(queryBuffer) - sizeof(SYSTEM_FIRMWARE_TABLE_INFORMATION);

    unsigned long providers[] = {
        MAKE_FOURCC('A', 'C', 'P', 'I'),
        0x41435049,
        MAKE_FOURCC('F', 'I', 'R', 'M')
    };

    unsigned long tableIds[] = {
        MAKE_FOURCC('i', 'B', 'F', 'T'),
        MAKE_FOURCC('I', 'B', 'F', 'T'),
        0x54464269,
        0x54464249
    };

    bool querySuccess = false;

    for (int pr = 0; pr < 3 && !querySuccess; pr++) {
        for (int tb = 0; tb < 4 && !querySuccess; tb++) {
            for (unsigned long act = 0; act <= 1 && !querySuccess; act++) {
                memset(queryBuffer, 0, sizeof(queryBuffer));
                pFirmware->ProviderSignature = providers[pr];
                pFirmware->Action = act;
                pFirmware->TableID = tableIds[tb];
                pFirmware->TableBufferLength = maxPayload;

                unsigned long returnLength = 0;
                NTSTATUS status = NtQuerySystemInformation(SystemFirmwareTableInformation, pFirmware, sizeof(queryBuffer), &returnLength);

                if (NT_SUCCESS(status) && pFirmware->TableBufferLength >= 48) {
                    querySuccess = true;
                    LogWriteA("[+] NtQuerySystemInformation SUCCESS! Provider=");
                    LogWriteHex(providers[pr]);
                    LogWriteA(" TableID=");
                    LogWriteHex(tableIds[tb]);
                    LogWriteA(" Len=");
                    LogWriteHex(pFirmware->TableBufferLength);
                    LogWriteA("\r\n");
                }
            }
        }
    }

    if (!querySuccess) {
        LogWriteA("[-] NtQuerySystemInformation did not return iBFT. Scanning Registry ACPI Tree...\r\n");

        const wchar_t* acpiRoots[] = {
            L"\\Registry\\Machine\\HARDWARE\\ACPI",
            L"\\Registry\\Machine\\HARDWARE\\DESCRIPTION\\System"
        };

        bool foundInReg = false;
        for (int r = 0; r < 2 && !foundInReg; r++) {
            UNICODE_STRING regPath;
            RtlInitUnicodeString(&regPath, acpiRoots[r]);
            OBJECT_ATTRIBUTES objAttr;
            objAttr.Length = sizeof(OBJECT_ATTRIBUTES);
            objAttr.RootDirectory = nullptr;
            objAttr.ObjectName = &regPath;
            objAttr.Attributes = OBJ_CASE_INSENSITIVE;
            objAttr.SecurityDescriptor = nullptr;
            objAttr.SecurityQualityOfService = nullptr;

            void* hAcpiRoot = nullptr;
            if (NT_SUCCESS(NtOpenKey(&hAcpiRoot, KEY_ENUMERATE_SUB_KEYS, &objAttr))) {
                unsigned char subKeyBuf[512];
                unsigned long resLen = 0;

                for (unsigned long idx = 0; idx < 32 && !foundInReg; idx++) {
                    NTSTATUS enumStatus = NtEnumerateKey(hAcpiRoot, idx, KeyBasicInformation, subKeyBuf, sizeof(subKeyBuf), &resLen);
                    if (!NT_SUCCESS(enumStatus)) break;

                    PKEY_BASIC_INFORMATION pSubInfo = (PKEY_BASIC_INFORMATION)subKeyBuf;
                    unsigned long nLenChars = pSubInfo->NameLength / sizeof(wchar_t);

                    wchar_t safeSubName[64] = {0};
                    unsigned long cpyCount = nLenChars < 63 ? nLenChars : 63;
                    for (unsigned long k = 0; k < cpyCount; k++) safeSubName[k] = pSubInfo->Name[k];
                    safeSubName[cpyCount] = L'\0';

                    wchar_t fullSubPath[256];
                    StrCopy(fullSubPath, acpiRoots[r], 256);
                    StrCat(fullSubPath, L"\\", 256);
                    StrCat(fullSubPath, safeSubName, 256);

                    LogWriteA("    - Checking ACPI Subkey: "); LogWriteW(fullSubPath); LogWriteA("\r\n");

                    if (StrEqual(safeSubName, L"iBFT") || StrEqual(safeSubName, L"IBFT") ||
                        StrStartsWith(safeSubName, L"iBFT") || StrStartsWith(safeSubName, L"IBFT")) {
                        
                        UNICODE_STRING subKeyName;
                        RtlInitUnicodeString(&subKeyName, fullSubPath);
                        OBJECT_ATTRIBUTES subAttr;
                        subAttr.Length = sizeof(OBJECT_ATTRIBUTES);
                        subAttr.RootDirectory = nullptr;
                        subAttr.ObjectName = &subKeyName;
                        subAttr.Attributes = OBJ_CASE_INSENSITIVE;
                        subAttr.SecurityDescriptor = nullptr;
                        subAttr.SecurityQualityOfService = nullptr;

                        void* hTableKey = nullptr;
                        if (NT_SUCCESS(NtOpenKey(&hTableKey, KEY_ENUMERATE_SUB_KEYS | KEY_QUERY_VALUE, &subAttr))) {
                            unsigned char childBuf[512];
                            unsigned long childResLen = 0;
                            for (unsigned long cIdx = 0; cIdx < 8 && !foundInReg; cIdx++) {
                                if (NT_SUCCESS(NtEnumerateKey(hTableKey, cIdx, KeyBasicInformation, childBuf, sizeof(childBuf), &childResLen))) {
                                    PKEY_BASIC_INFORMATION pChildInfo = (PKEY_BASIC_INFORMATION)childBuf;
                                    unsigned long cnLenChars = pChildInfo->NameLength / sizeof(wchar_t);
                                    wchar_t safeChildName[64] = {0};
                                    unsigned long cChildCount = cnLenChars < 63 ? cnLenChars : 63;
                                    for (unsigned long ck = 0; ck < cChildCount; ck++) safeChildName[ck] = pChildInfo->Name[ck];
                                    safeChildName[cChildCount] = L'\0';

                                    wchar_t childPath[256];
                                    StrCopy(childPath, fullSubPath, 256);
                                    StrCat(childPath, L"\\", 256);
                                    StrCat(childPath, safeChildName, 256);

                                    UNICODE_STRING childKeyName;
                                    RtlInitUnicodeString(&childKeyName, childPath);
                                    OBJECT_ATTRIBUTES childAttr;
                                    childAttr.Length = sizeof(OBJECT_ATTRIBUTES);
                                    childAttr.RootDirectory = nullptr;
                                    childAttr.ObjectName = &childKeyName;
                                    childAttr.Attributes = OBJ_CASE_INSENSITIVE;
                                    childAttr.SecurityDescriptor = nullptr;
                                    childAttr.SecurityQualityOfService = nullptr;

                                    void* hChild = nullptr;
                                    if (NT_SUCCESS(NtOpenKey(&hChild, KEY_QUERY_VALUE, &childAttr))) {
                                        UNICODE_STRING valName;
                                        RtlInitUnicodeString(&valName, L"00000000");
                                        static unsigned char tableValBuf[4096];
                                        unsigned long qLen = 0;
                                        if (NT_SUCCESS(NtQueryValueKey(hChild, &valName, KeyValuePartialInformation, tableValBuf, sizeof(tableValBuf), &qLen))) {
                                            PKEY_VALUE_PARTIAL_INFORMATION pPart = (PKEY_VALUE_PARTIAL_INFORMATION)tableValBuf;
                                            if (pPart->Type == REG_BINARY && pPart->DataLength >= 48) {
                                                unsigned long copyBytes = pPart->DataLength < maxPayload ? pPart->DataLength : maxPayload;
                                                memcpy(pFirmware->TableBuffer, pPart->Data, copyBytes);
                                                pFirmware->TableBufferLength = copyBytes;
                                                foundInReg = true;
                                                LogWriteA("[+] Found iBFT binary in Registry ACPI dump at: ");
                                                LogWriteW(childPath);
                                                LogWriteA("\r\n");
                                            }
                                        }
                                        NtClose(hChild);
                                    }
                                }
                            }
                            NtClose(hTableKey);
                        }
                    }
                }
                NtClose(hAcpiRoot);
            }
        }

        if (!foundInReg) {
            LogWriteA("[-] iBFT table not available in ACPI.\r\n");
            return false;
        }
    } else {
        LogWriteA("[+] Successfully retrieved iBFT table from ACPI Firmware!\r\n");
    }

    const unsigned char* table = pFirmware->TableBuffer;
    unsigned long tableLen = pFirmware->TableBufferLength;
    if (tableLen > maxPayload) {
        tableLen = maxPayload;
    }

    LogWriteA("[+] Scanning iBFT structures (Bounded Table Length: "); LogWriteHex(tableLen); LogWriteA(")...\r\n");

    // 1. Scan untuk NIC Block (StructureId = 3)
    bool nicFound = false;
    for (unsigned long i = 32; i + sizeof(IBFT_NIC) <= tableLen; i += 2) {
        IBFT_STRUCTURE_HEADER* hdr = (IBFT_STRUCTURE_HEADER*)(table + i);
        if (hdr->StructureId == 3 && hdr->Length >= 32 && hdr->Length <= 256) {
            IBFT_NIC* nic = (IBFT_NIC*)hdr;
            LogWriteA("[+] Found iBFT NIC Block at offset: "); LogWriteHex(i); LogWriteA("\r\n");

            if (FormatIpv4FromBytes(nic->IpAddress, outIp, 64)) {
                nicFound = true;
                PrefixToSubnetMask(nic->SubnetMaskPrefix, outMask, 64);
                FormatIpv4FromBytes(nic->Gateway, outGw, 64);
                FormatIpv4FromBytes(nic->PrimaryDns, outDns1, 64);
                FormatIpv4FromBytes(nic->SecondaryDns, outDns2, 64);

                LogWriteA("    -> IP Extracted: "); LogWriteW(outIp); LogWriteA("\r\n");
                LogWriteA("    -> Mask Extracted: "); LogWriteW(outMask); LogWriteA("\r\n");
                LogWriteA("    -> Gateway Extracted: "); LogWriteW(outGw); LogWriteA("\r\n");
                LogWriteA("    -> DNS1 Extracted: "); LogWriteW(outDns1); LogWriteA("\r\n");
                LogWriteA("    -> DNS2 Extracted: "); LogWriteW(outDns2); LogWriteA("\r\n");

                if (nic->HostNameOffset > 0 && (nic->HostNameOffset + nic->HostNameLength) <= tableLen) {
                    const char* rawHost = (const char*)(table + nic->HostNameOffset);
                    unsigned long cLen = nic->HostNameLength < 63 ? nic->HostNameLength : 63;
                    for (unsigned long k = 0; k < cLen; k++) outHost[k] = (wchar_t)rawHost[k];
                    outHost[cLen] = L'\0';
                    LogWriteA("    -> HostName Extracted from NIC: "); LogWriteW(outHost); LogWriteA("\r\n");
                }
                break;
            }
        }
    }

    // 2. Scan untuk Initiator Block (StructureId = 2) untuk HostName & IQN
    for (unsigned long i = 32; i + sizeof(IBFT_INITIATOR) <= tableLen; i += 2) {
        IBFT_STRUCTURE_HEADER* hdr = (IBFT_STRUCTURE_HEADER*)(table + i);
        if (hdr->StructureId == 2 && hdr->Length >= 16 && hdr->Length <= 256) {
            IBFT_INITIATOR* init = (IBFT_INITIATOR*)hdr;
            if (init->InitiatorNameOffset > 0 && (init->InitiatorNameOffset + init->InitiatorNameLength) <= tableLen) {
                const char* rawIqn = (const char*)(table + init->InitiatorNameOffset);
                unsigned long iqnLen = init->InitiatorNameLength;

                wchar_t initIqnW[128] = {0};
                unsigned long wLen = iqnLen < 127 ? iqnLen : 127;
                for (unsigned long k = 0; k < wLen; k++) initIqnW[k] = (wchar_t)rawIqn[k];
                initIqnW[wLen] = L'\0';
                LogWriteA("    -> Initiator IQN: "); LogWriteW(initIqnW); LogWriteA("\r\n");

                if (outHost[0] == L'\0') {
                    int colonIdx = -1;
                    for (unsigned long k = 0; k < iqnLen; k++) {
                        if (rawIqn[k] == ':') colonIdx = (int)k;
                    }
                    if (colonIdx != -1 && (unsigned long)(colonIdx + 1) < iqnLen) {
                        const char* subName = rawIqn + colonIdx + 1;
                        unsigned long subLen = iqnLen - (colonIdx + 1);
                        if (subLen > 63) subLen = 63;
                        for (unsigned long k = 0; k < subLen; k++) outHost[k] = (wchar_t)subName[k];
                        outHost[subLen] = L'\0';
                        LogWriteA("    -> HostName Extracted from Initiator IQN: "); LogWriteW(outHost); LogWriteA("\r\n");
                    }
                }
            }
            break;
        }
    }

    // 3. Scan untuk Target Block (StructureId = 4) untuk Target IQN
    for (unsigned long i = 32; i + sizeof(IBFT_TARGET) <= tableLen; i += 2) {
        IBFT_STRUCTURE_HEADER* hdr = (IBFT_STRUCTURE_HEADER*)(table + i);
        if (hdr->StructureId == 4 && hdr->Length >= 16 && hdr->Length <= 256) {
            IBFT_TARGET* tgt = (IBFT_TARGET*)hdr;
            if (tgt->TargetNameOffset > 0 && (tgt->TargetNameOffset + tgt->TargetNameLength) <= tableLen) {
                const char* rawTgt = (const char*)(table + tgt->TargetNameOffset);
                unsigned long tgtLen = tgt->TargetNameLength;

                wchar_t tgtIqnW[128] = {0};
                unsigned long wLen = tgtLen < 127 ? tgtLen : 127;
                for (unsigned long k = 0; k < wLen; k++) tgtIqnW[k] = (wchar_t)rawTgt[k];
                tgtIqnW[wLen] = L'\0';
                LogWriteA("    -> Target IQN: "); LogWriteW(tgtIqnW); LogWriteA("\r\n");

                if (outHost[0] == L'\0') {
                    int vhdIdx = -1;
                    for (unsigned long k = 0; k + 4 <= tgtLen; k++) {
                        if (rawTgt[k] == 'v' && rawTgt[k+1] == 'h' && rawTgt[k+2] == 'd' && rawTgt[k+3] == '-') {
                            vhdIdx = (int)(k + 4);
                            break;
                        }
                    }
                    if (vhdIdx != -1 && (unsigned long)vhdIdx < tgtLen) {
                        const char* subName = rawTgt + vhdIdx;
                        unsigned long subLen = tgtLen - vhdIdx;
                        if (subLen > 63) subLen = 63;
                        for (unsigned long k = 0; k < subLen; k++) outHost[k] = (wchar_t)subName[k];
                        outHost[subLen] = L'\0';
                        LogWriteA("    -> HostName Extracted from Target IQN: "); LogWriteW(outHost); LogWriteA("\r\n");
                    }
                }
            }
            break;
        }
    }

    return nicFound && IsValidIp(outIp);
}

// -------------------------------------------------------------
// Entry Point: Native Subsystem Application (BootExecute)
// -------------------------------------------------------------

extern "C" void NtProcessStartup(void* Peb) {
    (void)Peb;
    LogOpen();
    LogWriteA("================================================================\r\n");
    LogWriteA(" [Simple-Iscsi BootHelper] Started in BootExecute (Safe Mode)\r\n");
    LogWriteA("================================================================\r\n");

    wchar_t rawHostName[64] = {0};
    wchar_t targetIp[64] = {0};
    wchar_t subnetMask[64] = {0};
    wchar_t gatewayIp[64] = {0};
    wchar_t dns1[64] = {0};
    wchar_t dns2[64] = {0};

    // 1. Baca langsung dari iBFT (ACPI Firmware / Driverless) secara aman
    bool gotIbft = ReadParametersFromIBFT(rawHostName, targetIp, subnetMask, gatewayIp, dns1, dns2);

    if (gotIbft) {
        LogWriteA("[+] Successfully retrieved parameters from iBFT (Driverless):\r\n");
        LogWriteA("    - HostName   : "); LogWriteW(rawHostName); LogWriteA("\r\n");
        LogWriteA("    - Target IP  : "); LogWriteW(targetIp); LogWriteA("\r\n");
        LogWriteA("    - SubnetMask : "); LogWriteW(subnetMask); LogWriteA("\r\n");
        LogWriteA("    - GatewayIP  : "); LogWriteW(gatewayIp); LogWriteA("\r\n");
        LogWriteA("    - DNS1       : "); LogWriteW(dns1); LogWriteA("\r\n");
        LogWriteA("    - DNS2       : "); LogWriteW(dns2); LogWriteA("\r\n");
    } else {
        LogWriteA("[-] iBFT not detected in ACPI Firmware.\r\n");
    }

    // Fallback Gateway jika GatewayIP kosong/0.0.0.0
    if (!IsValidIp(gatewayIp) && IsValidIp(targetIp)) {
        StrCopy(gatewayIp, targetIp, 64);
        int lastDotIdx = -1;
        for (int i = 0; gatewayIp[i] != L'\0'; i++) {
            if (gatewayIp[i] == L'.') lastDotIdx = i;
        }
        if (lastDotIdx != -1) {
            gatewayIp[lastDotIdx + 1] = L'1';
            gatewayIp[lastDotIdx + 2] = L'\0';
        }
        LogWriteA("    - Auto Gateway Generated: "); LogWriteW(gatewayIp); LogWriteA("\r\n");
    }

    // Fallback DNS jika DNS1/DNS2 kosong/0.0.0.0
    if (!IsValidIp(dns1)) {
        if (IsValidIp(gatewayIp)) {
            StrCopy(dns1, gatewayIp, 64);
        } else {
            StrCopy(dns1, L"1.1.1.1", 64);
        }
        LogWriteA("    - Auto DNS1 Generated: "); LogWriteW(dns1); LogWriteA("\r\n");
    }
    if (!IsValidIp(dns2)) {
        StrCopy(dns2, L"8.8.8.8", 64);
        LogWriteA("    - Auto DNS2 Generated: "); LogWriteW(dns2); LogWriteA("\r\n");
    }

    // Gabungkan DNS1 dan DNS2 ke format string DNS standar ("192.168.180.1,8.8.8.8")
    wchar_t combinedDns[128] = {0};
    StrCopy(combinedDns, dns1, 128);
    if (IsValidIp(dns2) && !StrEqual(dns1, dns2)) {
        StrCat(combinedDns, L",", 128);
        StrCat(combinedDns, dns2, 128);
    }

    // 2. Tulis penanda SimpleIscsiBoot untuk pembersihan IP & sinkronisasi user-mode
    if (IsValidIp(targetIp)) {
        LogWriteA("[+] Writing SimpleIscsiBoot Marker for user-mode service...\r\n");
        WriteSimpleIscsiMarker(targetIp, gatewayIp, subnetMask, dns1, dns2, combinedDns, rawHostName);

        // 3. Selaraskan DNS & bersihkan 0.0.0.0 Gateway di registri
        SanitizeDnsAndGateway(gatewayIp, combinedDns);
    }

    // 4. Sinkronkan Hostname (Nama PC) Murni dari DHCP Option 12 / iBFT
    if (rawHostName[0] != L'\0') {
        wchar_t formattedHost[64] = {0};
        FormatCleanHostname(rawHostName, formattedHost, 64);
        unsigned long hostByteLen = (StrLen(formattedHost) + 1) * sizeof(wchar_t);

        LogWriteA("[+] Synchronizing ComputerName: "); LogWriteW(formattedHost); LogWriteA("\r\n");

        OBJECT_ATTRIBUTES objAttr;
        objAttr.Length = sizeof(OBJECT_ATTRIBUTES);
        objAttr.RootDirectory = nullptr;
        objAttr.Attributes = OBJ_CASE_INSENSITIVE;
        objAttr.SecurityDescriptor = nullptr;
        objAttr.SecurityQualityOfService = nullptr;

        // A. ComputerName\ComputerName
        UNICODE_STRING cnPath;
        RtlInitUnicodeString(&cnPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Control\\ComputerName\\ComputerName");
        objAttr.ObjectName = &cnPath;
        void* hKey = nullptr;
        if (NT_SUCCESS(NtOpenKey(&hKey, KEY_SET_VALUE, &objAttr))) {
            UNICODE_STRING valName;
            RtlInitUnicodeString(&valName, L"ComputerName");
            NtSetValueKey(hKey, &valName, 0, REG_SZ, formattedHost, hostByteLen);
            NtClose(hKey);
        }

        // B. ComputerName\ActiveComputerName
        UNICODE_STRING acnPath;
        RtlInitUnicodeString(&acnPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Control\\ComputerName\\ActiveComputerName");
        objAttr.ObjectName = &acnPath;
        if (NT_SUCCESS(NtOpenKey(&hKey, KEY_SET_VALUE, &objAttr))) {
            UNICODE_STRING valName;
            RtlInitUnicodeString(&valName, L"ComputerName");
            NtSetValueKey(hKey, &valName, 0, REG_SZ, formattedHost, hostByteLen);
            NtClose(hKey);
        }

        // C. Tcpip\Parameters (Hostname & NV Hostname)
        UNICODE_STRING tcpipPath;
        RtlInitUnicodeString(&tcpipPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\Tcpip\\Parameters");
        objAttr.ObjectName = &tcpipPath;
        if (NT_SUCCESS(NtOpenKey(&hKey, KEY_SET_VALUE, &objAttr))) {
            UNICODE_STRING valHost;
            RtlInitUnicodeString(&valHost, L"Hostname");
            NtSetValueKey(hKey, &valHost, 0, REG_SZ, formattedHost, hostByteLen);

            UNICODE_STRING valNvHost;
            RtlInitUnicodeString(&valNvHost, L"NV Hostname");
            NtSetValueKey(hKey, &valNvHost, 0, REG_SZ, formattedHost, hostByteLen);

            NtClose(hKey);
        }
    }

    // 5. Konfigurasi Parameter iScsiPrt (WaitForNetworkAtBoot & DelayForNetworkAtBoot) untuk Fast-Boot
    ConfigureIscsiPrtParameters();

    LogWriteA("================================================================\r\n");
    LogWriteA(" [Simple-Iscsi BootHelper] Completed Safely\r\n");
    LogWriteA("================================================================\r\n");
    LogClose();

    // Keluar proses secara bersih ke Session Manager
    NtTerminateProcess((void*)-1, 0);
}
