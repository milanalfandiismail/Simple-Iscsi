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

#define REG_SZ 1
#define REG_DWORD 4
#define REG_MULTI_SZ 7

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

// Helper String & Network Utilities (No CRT)

bool StrEqual(const wchar_t* a, const wchar_t* b) {
    if (!a || !b) return false;
    while (*a && *b) {
        if (*a != *b) return false;
        a++;
        b++;
    }
    return *a == *b;
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

// Format string ke REG_MULTI_SZ (diakhiri double null: "192.168.180.3\0\0")
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

// Membaca REG_SZ string value dari key registry
bool ReadRegString(void* hKey, const wchar_t* valNameStr, wchar_t* outBuf, unsigned long maxChars) {
    if (!hKey || !valNameStr || !outBuf || maxChars == 0) return false;
    outBuf[0] = L'\0';

    UNICODE_STRING valName;
    RtlInitUnicodeString(&valName, valNameStr);

    unsigned char queryBuf[256] = {0};
    unsigned long qLen = 0;
    if (NT_SUCCESS(NtQueryValueKey(hKey, &valName, KeyValuePartialInformation, queryBuf, sizeof(queryBuf), &qLen))) {
        PKEY_VALUE_PARTIAL_INFORMATION pValInfo = (PKEY_VALUE_PARTIAL_INFORMATION)queryBuf;
        if (pValInfo->Type == REG_SZ && pValInfo->DataLength > 0) {
            wchar_t* rawStr = (wchar_t*)pValInfo->Data;
            unsigned long rawLenChars = pValInfo->DataLength / sizeof(wchar_t);
            if (rawLenChars > 0 && rawStr[rawLenChars - 1] == L'\0') rawLenChars--;

            unsigned long copyLen = rawLenChars < (maxChars - 1) ? rawLenChars : (maxChars - 1);
            for (unsigned long k = 0; k < copyLen; k++) {
                outBuf[k] = rawStr[k];
            }
            outBuf[copyLen] = L'\0';
            return true;
        }
    }
    return false;
}

// Memastikan HostName memiliki akhiran "TM"
void FormatHostnameWithTm(const wchar_t* inHost, wchar_t* outHost, unsigned long maxChars) {
    if (!inHost || inHost[0] == L'\0') {
        StrCopy(outHost, L"PC-CLIENTTM", maxChars);
        return;
    }

    StrCopy(outHost, inHost, maxChars);
    unsigned long len = StrLen(outHost);

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

// -------------------------------------------------------------
// Entry Point: Native Subsystem Application (BootExecute)
// -------------------------------------------------------------

extern "C" void NtProcessStartup(void* Peb) {
    // 1. Buka Parameters iSharePnp
    UNICODE_STRING isharePath;
    RtlInitUnicodeString(&isharePath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\iSharePnp\\Parameters");

    OBJECT_ATTRIBUTES objAttr;
    objAttr.Length = sizeof(OBJECT_ATTRIBUTES);
    objAttr.RootDirectory = nullptr;
    objAttr.ObjectName = &isharePath;
    objAttr.Attributes = OBJ_CASE_INSENSITIVE;
    objAttr.SecurityDescriptor = nullptr;
    objAttr.SecurityQualityOfService = nullptr;

    void* hIshareKey = nullptr;
    NTSTATUS status = NtOpenKey(&hIshareKey, KEY_QUERY_VALUE, &objAttr);

    wchar_t rawHostName[64] = {0};
    wchar_t targetIp[64] = {0};
    wchar_t subnetMask[64] = {0};
    wchar_t gatewayIp[64] = {0};
    wchar_t dns1[64] = {0};
    wchar_t dns2[64] = {0};

    if (NT_SUCCESS(status)) {
        ReadRegString(hIshareKey, L"HostName", rawHostName, 64);
        
        // Baca BindIP (fallback ke DHCP jika BindIP kosong)
        if (!ReadRegString(hIshareKey, L"BindIP", targetIp, 64) || !IsValidIp(targetIp)) {
            ReadRegString(hIshareKey, L"DHCP", targetIp, 64);
        }

        ReadRegString(hIshareKey, L"Mask", subnetMask, 64);
        if (!IsValidIp(subnetMask)) {
            StrCopy(subnetMask, L"255.255.255.0", 64); // Default Subnet Mask
        }

        ReadRegString(hIshareKey, L"GatewayIP", gatewayIp, 64);
        ReadRegString(hIshareKey, L"Dns1", dns1, 64);
        ReadRegString(hIshareKey, L"Dns2", dns2, 64);

        NtClose(hIshareKey);
    }

    // 2. Jika IP Target valid, pasang konfigurasi IP Statis (Disable DHCP) ke seluruh Interfaces
    if (IsValidIp(targetIp)) {
        // Buffer Multi-SZ
        wchar_t multiSzIp[64];
        unsigned long ipByteLen = BuildMultiSz(targetIp, multiSzIp, 64);

        wchar_t multiSzMask[64];
        unsigned long maskByteLen = BuildMultiSz(subnetMask, multiSzMask, 64);

        wchar_t multiSzGateway[64];
        unsigned long gwByteLen = 0;
        if (IsValidIp(gatewayIp)) {
            gwByteLen = BuildMultiSz(gatewayIp, multiSzGateway, 64);
        }

        wchar_t combinedDns[128] = {0};
        if (IsValidIp(dns1)) {
            StrCopy(combinedDns, dns1, 128);
            if (IsValidIp(dns2)) {
                StrCat(combinedDns, L",", 128);
                StrCat(combinedDns, dns2, 128);
            }
        } else if (IsValidIp(dns2)) {
            StrCopy(combinedDns, dns2, 128);
        }
        unsigned long dnsByteLen = (StrLen(combinedDns) + 1) * sizeof(wchar_t);

        // Buka root Interfaces registry
        UNICODE_STRING interfacesPath;
        RtlInitUnicodeString(&interfacesPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces");
        objAttr.ObjectName = &interfacesPath;

        void* hInterfacesKey = nullptr;
        if (NT_SUCCESS(NtOpenKey(&hInterfacesKey, KEY_ENUMERATE_SUB_KEYS | KEY_QUERY_VALUE, &objAttr))) {
            unsigned char enumBuffer[512];
            unsigned long resultLength = 0;

            for (unsigned long index = 0; ; index++) {
                status = NtEnumerateKey(hInterfacesKey, index, KeyBasicInformation, enumBuffer, sizeof(enumBuffer), &resultLength);
                if (!NT_SUCCESS(status)) break;

                PKEY_BASIC_INFORMATION pKeyInfo = (PKEY_BASIC_INFORMATION)enumBuffer;
                unsigned long nameLenChars = pKeyInfo->NameLength / sizeof(wchar_t);

                wchar_t subKeyPath[256];
                StrCopy(subKeyPath, L"\\Registry\\Machine\\System\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\", 256);
                unsigned long baseLen = StrLen(subKeyPath);

                for (unsigned long i = 0; i < nameLenChars && (baseLen + i + 1) < 256; i++) {
                    subKeyPath[baseLen + i] = pKeyInfo->Name[i];
                }
                subKeyPath[baseLen + (nameLenChars < (256 - baseLen - 1) ? nameLenChars : (256 - baseLen - 1))] = L'\0';

                UNICODE_STRING subKeyName;
                RtlInitUnicodeString(&subKeyName, subKeyPath);

                OBJECT_ATTRIBUTES subObjAttr;
                subObjAttr.Length = sizeof(OBJECT_ATTRIBUTES);
                subObjAttr.RootDirectory = nullptr;
                subObjAttr.ObjectName = &subKeyName;
                subObjAttr.Attributes = OBJ_CASE_INSENSITIVE;
                subObjAttr.SecurityDescriptor = nullptr;
                subObjAttr.SecurityQualityOfService = nullptr;

                void* hSubKey = nullptr;
                if (NT_SUCCESS(NtOpenKey(&hSubKey, KEY_SET_VALUE, &subObjAttr))) {
                    // 1. Matikan DHCP: EnableDHCP = 0
                    UNICODE_STRING valEnableDhcp;
                    RtlInitUnicodeString(&valEnableDhcp, L"EnableDHCP");
                    unsigned long enableDhcpVal = 0; // 0 = Static IP
                    NtSetValueKey(hSubKey, &valEnableDhcp, 0, REG_DWORD, &enableDhcpVal, sizeof(enableDhcpVal));

                    // 2. Set IPAddress Statis
                    UNICODE_STRING valIp;
                    RtlInitUnicodeString(&valIp, L"IPAddress");
                    NtSetValueKey(hSubKey, &valIp, 0, REG_MULTI_SZ, multiSzIp, ipByteLen);

                    // 3. Set SubnetMask Statis
                    UNICODE_STRING valMask;
                    RtlInitUnicodeString(&valMask, L"SubnetMask");
                    NtSetValueKey(hSubKey, &valMask, 0, REG_MULTI_SZ, multiSzMask, maskByteLen);

                    // 4. Set DefaultGateway Statis
                    if (gwByteLen > 0) {
                        UNICODE_STRING valGw;
                        RtlInitUnicodeString(&valGw, L"DefaultGateway");
                        NtSetValueKey(hSubKey, &valGw, 0, REG_MULTI_SZ, multiSzGateway, gwByteLen);
                    }

                    // 5. Set NameServer (DNS)
                    if (dnsByteLen > sizeof(wchar_t)) {
                        UNICODE_STRING valDns;
                        RtlInitUnicodeString(&valDns, L"NameServer");
                        NtSetValueKey(hSubKey, &valDns, 0, REG_SZ, combinedDns, dnsByteLen);
                    }

                    NtClose(hSubKey);
                }
            }
            NtClose(hInterfacesKey);
        }
    }

    // 3. Sinkronkan Hostname (Nama PC)
    if (rawHostName[0] != L'\0') {
        wchar_t formattedHost[64] = {0};
        FormatHostnameWithTm(rawHostName, formattedHost, 64);
        unsigned long hostByteLen = (StrLen(formattedHost) + 1) * sizeof(wchar_t);

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

    // Keluar proses secara bersih
    NtTerminateProcess((void*)-1, 0);
}
