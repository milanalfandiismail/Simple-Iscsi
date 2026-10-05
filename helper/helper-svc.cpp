#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winsock2.h>
#include <ws2ipdef.h>
#include <iphlpapi.h>
#include <netioapi.h>
#include <stdio.h>
#include <stdlib.h>

#pragma comment(lib, "iphlpapi.lib")
#pragma comment(lib, "ws2_32.lib")
#pragma comment(lib, "advapi32.lib")

#define SERVICE_NAME "SimpleIscsiHelper"

FILE* g_logFile = nullptr;

void LogA(const char* fmt, ...) {
    if (!g_logFile) {
        fopen_s(&g_logFile, "C:\\helper-svc.log", "a");
        if (!g_logFile) {
            fopen_s(&g_logFile, "C:\\Windows\\System32\\helper-svc.log", "a");
        }
    }
    if (g_logFile) {
        va_list args;
        va_start(args, fmt);
        vfprintf(g_logFile, fmt, args);
        va_end(args);
        fflush(g_logFile);
    }
}

// Format IPv4 sockaddr ke string "a.b.c.d"
void FormatIpv4(const IN_ADDR* addr, char* outBuf, size_t bufSize) {
    unsigned char* b = (unsigned char*)addr;
    sprintf_s(outBuf, bufSize, "%u.%u.%u.%u", b[0], b[1], b[2], b[3]);
}

// Baca Parameter Booting dari HKLM\SYSTEM\CurrentControlSet\Services\SimpleIscsiBoot
bool ReadBootParameters(
    char* outIp, size_t maxIpLen,
    char* outGw, size_t maxGwLen,
    char* outDns, size_t maxDnsLen,
    wchar_t* outHost, size_t maxHostLen
) {
    HKEY hKey = nullptr;
    if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SYSTEM\\CurrentControlSet\\Services\\SimpleIscsiBoot", 0, KEY_READ, &hKey) == ERROR_SUCCESS) {
        if (outIp && maxIpLen > 0) {
            char buf[64] = {0};
            DWORD bufSize = sizeof(buf);
            DWORD type = REG_SZ;
            if (RegQueryValueExA(hKey, "TargetIp", nullptr, &type, (LPBYTE)buf, &bufSize) == ERROR_SUCCESS && buf[0] != '\0') {
                strcpy_s(outIp, maxIpLen, buf);
            }
        }
        if (outGw && maxGwLen > 0) {
            char buf[64] = {0};
            DWORD bufSize = sizeof(buf);
            DWORD type = REG_SZ;
            if (RegQueryValueExA(hKey, "GatewayIp", nullptr, &type, (LPBYTE)buf, &bufSize) == ERROR_SUCCESS && buf[0] != '\0') {
                strcpy_s(outGw, maxGwLen, buf);
            }
        }
        if (outDns && maxDnsLen > 0) {
            char buf[128] = {0};
            DWORD bufSize = sizeof(buf);
            DWORD type = REG_SZ;
            if (RegQueryValueExA(hKey, "NameServer", nullptr, &type, (LPBYTE)buf, &bufSize) == ERROR_SUCCESS && buf[0] != '\0') {
                strcpy_s(outDns, maxDnsLen, buf);
            }
        }
        if (outHost && maxHostLen > 0) {
            wchar_t wbuf[64] = {0};
            DWORD wbufSize = sizeof(wbuf);
            DWORD type = REG_SZ;
            if (RegQueryValueExW(hKey, L"Hostname", nullptr, &type, (LPBYTE)wbuf, &wbufSize) == ERROR_SUCCESS && wbuf[0] != L'\0') {
                wcscpy_s(outHost, maxHostLen, wbuf);
            }
        }
        RegCloseKey(hKey);
        return (outIp && outIp[0] != '\0');
    }
    return false;
}

// Fallback: Baca Target IP dari ACPI iBFT Firmware Table
bool ReadTargetIpFromFirmware(char* outIp, size_t maxLen, char* outGw, size_t maxGwLen, char* outDns, size_t maxDnsLen) {
    DWORD sig = 'TFBi'; // 'iBFT'
    DWORD bufSize = GetSystemFirmwareTable('ACPI', sig, nullptr, 0);
    if (bufSize == 0) return false;

    BYTE* pBuf = (BYTE*)malloc(bufSize);
    if (!pBuf) return false;

    if (GetSystemFirmwareTable('ACPI', sig, pBuf, bufSize) == bufSize) {
        // Cari block NIC di iBFT (StructureId = 3)
        for (DWORD offset = 32; offset + 48 <= bufSize; offset += 2) {
            if (pBuf[offset] == 0x03) { // NIC structure ID
                BYTE* ipBytes = &pBuf[offset + 4];
                BYTE* gwBytes = &pBuf[offset + 20];
                BYTE* dns1Bytes = &pBuf[offset + 36];
                
                // Periksa Target IPv4
                if (ipBytes[12] != 0 && ipBytes[12] != 127) {
                    sprintf_s(outIp, maxLen, "%u.%u.%u.%u", ipBytes[12], ipBytes[13], ipBytes[14], ipBytes[15]);
                } else if (ipBytes[0] != 0 && ipBytes[0] != 127) {
                    sprintf_s(outIp, maxLen, "%u.%u.%u.%u", ipBytes[0], ipBytes[1], ipBytes[2], ipBytes[3]);
                }

                // Periksa Gateway
                if (gwBytes[12] != 0 && gwBytes[12] != 127) {
                    sprintf_s(outGw, maxGwLen, "%u.%u.%u.%u", gwBytes[12], gwBytes[13], gwBytes[14], gwBytes[15]);
                } else if (gwBytes[0] != 0 && gwBytes[0] != 127) {
                    sprintf_s(outGw, maxGwLen, "%u.%u.%u.%u", gwBytes[0], gwBytes[1], gwBytes[2], gwBytes[3]);
                }

                // Periksa DNS1
                if (dns1Bytes[12] != 0 && dns1Bytes[12] != 127) {
                    sprintf_s(outDns, maxDnsLen, "%u.%u.%u.%u", dns1Bytes[12], dns1Bytes[13], dns1Bytes[14], dns1Bytes[15]);
                } else if (dns1Bytes[0] != 0 && dns1Bytes[0] != 127) {
                    sprintf_s(outDns, maxDnsLen, "%u.%u.%u.%u", dns1Bytes[0], dns1Bytes[1], dns1Bytes[2], dns1Bytes[3]);
                }

                free(pBuf);
                return (outIp[0] != '\0');
            }
        }
    }
    free(pBuf);
    return false;
}

// Menyelaraskan DNS dan membersihkan DefaultGateway 0.0.0.0 di Registry
void AlignDnsAndGatewayInRegistry(const char* gwIp, const char* dnsStr) {
    if ((!gwIp || gwIp[0] == '\0' || strcmp(gwIp, "0.0.0.0") == 0) && (!dnsStr || dnsStr[0] == '\0')) {
        return;
    }

    LogA("[+] Aligning DNS and Gateway in Registry...\n");

    // 1. Tulis global DNS ke Tcpip\Parameters
    if (dnsStr && dnsStr[0] != '\0') {
        HKEY hTcpip = nullptr;
        if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters", 0, KEY_SET_VALUE, &hTcpip) == ERROR_SUCCESS) {
            RegSetValueExA(hTcpip, "NameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
            RegSetValueExA(hTcpip, "DhcpNameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
            RegCloseKey(hTcpip);
            LogA("    [+] Global NameServer updated: %s\n", dnsStr);
        }
    }

    // 2. Iterasi subkey di bawah Tcpip\Parameters\Interfaces
    HKEY hInterfaces = nullptr;
    if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces", 0, KEY_READ, &hInterfaces) == ERROR_SUCCESS) {
        char guidName[128];
        DWORD guidLen = sizeof(guidName);

        for (DWORD idx = 0; RegEnumKeyExA(hInterfaces, idx, guidName, &guidLen, nullptr, nullptr, nullptr, nullptr) == ERROR_SUCCESS; idx++) {
            guidLen = sizeof(guidName);

            char subPath[256];
            sprintf_s(subPath, sizeof(subPath), "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\%s", guidName);

            HKEY hSub = nullptr;
            if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, subPath, 0, KEY_READ | KEY_SET_VALUE, &hSub) == ERROR_SUCCESS) {
                // Tulis NameServer pada interface
                if (dnsStr && dnsStr[0] != '\0') {
                    RegSetValueExA(hSub, "NameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
                    RegSetValueExA(hSub, "DhcpNameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
                }

                // Cek DefaultGateway lama
                char existingGw[128] = {0};
                DWORD gwSize = sizeof(existingGw);
                DWORD type = 0;
                if (RegQueryValueExA(hSub, "DefaultGateway", nullptr, &type, (LPBYTE)existingGw, &gwSize) == ERROR_SUCCESS) {
                    if (strcmp(existingGw, "0.0.0.0") == 0 || strncmp(existingGw, "0.0.0.0", 7) == 0 || existingGw[0] == '\0') {
                        if (gwIp && gwIp[0] != '\0' && strcmp(gwIp, "0.0.0.0") != 0) {
                            char multiSzGw[64] = {0};
                            size_t gLen = strlen(gwIp);
                            memcpy(multiSzGw, gwIp, gLen);
                            multiSzGw[gLen] = '\0';
                            multiSzGw[gLen + 1] = '\0';

                            RegSetValueExA(hSub, "DefaultGateway", 0, REG_MULTI_SZ, (const BYTE*)multiSzGw, (DWORD)gLen + 2);
                            const char cleanMetric[] = "0\0\0";
                            RegSetValueExA(hSub, "DefaultGatewayMetric", 0, REG_MULTI_SZ, (const BYTE*)cleanMetric, sizeof(cleanMetric));
                            LogA("    [+] Fixed DefaultGateway on %s -> %s\n", guidName, gwIp);
                        }
                    }
                }
                RegCloseKey(hSub);
            }
        }
        RegCloseKey(hInterfaces);
    }
}

// Membersihkan rute bogus 0.0.0.0 di Kernel Routing Table
void CleanBogusDefaultRoutes() {
    PMIB_IPFORWARD_TABLE2 pTable = nullptr;
    if (GetIpForwardTable2(AF_INET, &pTable) == NO_ERROR && pTable) {
        for (ULONG i = 0; i < pTable->NumEntries; i++) {
            MIB_IPFORWARD_ROW2 row = pTable->Table[i];
            if (row.DestinationPrefix.Prefix.si_family == AF_INET && row.DestinationPrefix.PrefixLength == 0) {
                char nhStr[64] = {0};
                FormatIpv4(&row.NextHop.Ipv4.sin_addr, nhStr, sizeof(nhStr));

                // Jika NextHop adalah 0.0.0.0, hapus rute palsu ini
                if (strcmp(nhStr, "0.0.0.0") == 0) {
                    LogA("    [!] DETECTED BOGUS DEFAULT ROUTE via 0.0.0.0 (Interface: %lu). Deleting...\n", row.InterfaceIndex);
                    DWORD delRes = DeleteIpForwardEntry2(&row);
                    if (delRes == NO_ERROR) {
                        LogA("        [SUCCESS] Deleted bogus 0.0.0.0 route from kernel table.\n");
                    } else {
                        LogA("        [-] DeleteIpForwardEntry2 returned %lu\n", delRes);
                    }
                }
            }
        }
        FreeMibTable(pTable);
    }
}

// Pembersihan IP Unicast yang tidak sesuai Target IP
int ExecuteIpPurge() {
    LogA("================================================================\n");
    LogA(" [Simple-Iscsi Helper-Svc] User-Mode Network Alignment Started\n");
    LogA("================================================================\n");

    char targetIp[64] = {0};
    char gatewayIp[64] = {0};
    char nameServer[128] = {0};
    wchar_t hostName[64] = {0};

    if (ReadBootParameters(targetIp, sizeof(targetIp), gatewayIp, sizeof(gatewayIp), nameServer, sizeof(nameServer), hostName, 64)) {
        LogA("[+] Retrieved Boot Parameters from SimpleIscsiBoot:\n");
        LogA("    - Target IP : %s\n", targetIp);
        LogA("    - Gateway IP: %s\n", gatewayIp);
        LogA("    - DNS       : %s\n", nameServer);
    } else if (ReadTargetIpFromFirmware(targetIp, sizeof(targetIp), gatewayIp, sizeof(gatewayIp), nameServer, sizeof(nameServer))) {
        LogA("[+] Retrieved Boot Parameters from ACPI iBFT Firmware:\n");
        LogA("    - Target IP : %s\n", targetIp);
        LogA("    - Gateway IP: %s\n", gatewayIp);
        LogA("    - DNS       : %s\n", nameServer);
    } else {
        LogA("[-] Warning: Target IP not defined in Registry or iBFT.\n");
    }

    // Auto Gateway fallback jika gateway kosong/0.0.0.0
    if ((gatewayIp[0] == '\0' || strcmp(gatewayIp, "0.0.0.0") == 0) && targetIp[0] != '\0') {
        strcpy_s(gatewayIp, sizeof(gatewayIp), targetIp);
        char* lastDot = strrchr(gatewayIp, '.');
        if (lastDot) {
            *(lastDot + 1) = '1';
            *(lastDot + 2) = '\0';
        }
    }

    // Auto DNS fallback jika DNS kosong/0.0.0.0
    if (nameServer[0] == '\0' || strcmp(nameServer, "0.0.0.0") == 0) {
        if (gatewayIp[0] != '\0') {
            sprintf_s(nameServer, sizeof(nameServer), "%s,8.8.8.8", gatewayIp);
        } else {
            strcpy_s(nameServer, sizeof(nameServer), "1.1.1.1,8.8.8.8");
        }
    }

    // 1. Selaraskan DNS dan Gateway di Registry
    AlignDnsAndGatewayInRegistry(gatewayIp, nameServer);

    // 2. Bersihkan rute bogus 0.0.0.0 dari Kernel Routing Table
    CleanBogusDefaultRoutes();

    // 3. Sinkronisasi Hostname di User-Mode jika ada
    if (hostName[0] != L'\0') {
        SetComputerNameExW(ComputerNamePhysicalDnsHostname, hostName);
        SetComputerNameExW(ComputerNameNetBIOS, hostName);
        LogA("[+] User-Mode ComputerName synced to: %ls\n", hostName);
    }

    if (targetIp[0] == '\0') {
        return 0;
    }

    // 4. Inisialisasi Winsock & Scan Unicast IPs
    WSADATA wsaData;
    WSAStartup(MAKEWORD(2, 2), &wsaData);

    PMIB_UNICASTIPADDRESS_TABLE pTable = nullptr;
    DWORD status = GetUnicastIpAddressTable(AF_INET, &pTable);
    if (status != NO_ERROR || !pTable) {
        LogA("[-] GetUnicastIpAddressTable failed (Error: %lu)\n", status);
        WSACleanup();
        return 1;
    }

    LogA("[+] Scanning %lu active IPv4 Unicast addresses...\n", pTable->NumEntries);

    int purgedCount = 0;
    int keptCount = 0;

    for (DWORD i = 0; i < pTable->NumEntries; i++) {
        MIB_UNICASTIPADDRESS_ROW row = pTable->Table[i];
        if (row.Address.si_family == AF_INET) {
            char ipStr[64] = {0};
            FormatIpv4(&row.Address.Ipv4.sin_addr, ipStr, sizeof(ipStr));

            // Skip loopback dan APIPA
            if (strcmp(ipStr, "127.0.0.1") == 0 || strncmp(ipStr, "169.254.", 8) == 0) {
                LogA("    [*] Skipped system IP: %s\n", ipStr);
                continue;
            }

            if (strcmp(ipStr, targetIp) == 0) {
                LogA("    [+] KEEP Valid Target IP: %s (InterfaceIndex: %lu)\n", ipStr, row.InterfaceIndex);
                keptCount++;
            } else {
                LogA("    [!] DETECTED STALE/DUPLICATE IP: %s (InterfaceIndex: %lu)\n", ipStr, row.InterfaceIndex);
                LogA("        -> Purging stale IP via DeleteUnicastIpAddressEntry...\n");

                DWORD delStatus = DeleteUnicastIpAddressEntry(&row);
                if (delStatus == NO_ERROR) {
                    LogA("        [SUCCESS] Stale IP %s successfully deleted from TCP/IP stack!\n", ipStr);
                    purgedCount++;
                } else {
                    LogA("        [FAILED] DeleteUnicastIpAddressEntry failed (Error: %lu)\n", delStatus);
                }
            }
        }
    }

    FreeMibTable(pTable);
    WSACleanup();

    LogA("[+] IP Purge Complete: %d Stale IP(s) Removed, %d Target IP(s) Retained.\n", purgedCount, keptCount);
    LogA("================================================================\n");
    LogA(" [Simple-Iscsi Helper-Svc] Finished Successfully\n");
    LogA("================================================================\n\n");

    if (g_logFile) {
        fclose(g_logFile);
        g_logFile = nullptr;
    }

    return 0;
}

// Windows Service Handling
SERVICE_STATUS        g_ServiceStatus;
SERVICE_STATUS_HANDLE g_StatusHandle = nullptr;

VOID WINAPI ServiceCtrlHandler(DWORD CtrlCode) {
    if (CtrlCode == SERVICE_CONTROL_STOP || CtrlCode == SERVICE_CONTROL_SHUTDOWN) {
        g_ServiceStatus.dwCurrentState = SERVICE_STOPPED;
        SetServiceStatus(g_StatusHandle, &g_ServiceStatus);
    }
}

VOID WINAPI ServiceMain(DWORD argc, LPSTR* argv) {
    (void)argc; (void)argv;
    g_StatusHandle = RegisterServiceCtrlHandlerA(SERVICE_NAME, ServiceCtrlHandler);
    if (!g_StatusHandle) return;

    g_ServiceStatus.dwServiceType = SERVICE_WIN32_OWN_PROCESS;
    g_ServiceStatus.dwCurrentState = SERVICE_RUNNING;
    g_ServiceStatus.dwControlsAccepted = SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN;
    g_ServiceStatus.dwWin32ExitCode = 0;
    SetServiceStatus(g_StatusHandle, &g_ServiceStatus);

    // Jalankan pembersihan IP, DNS, dan Gateway
    ExecuteIpPurge();

    // Selesai bekerja, service langsung berhenti dengan bersih (0 RAM overhead)
    g_ServiceStatus.dwCurrentState = SERVICE_STOPPED;
    SetServiceStatus(g_StatusHandle, &g_ServiceStatus);
}

int main(int argc, char* argv[]) {
    // Jika dijalankan manual atau via Run key (/run), jalankan langsung
    if (argc > 1 && (_stricmp(argv[1], "/run") == 0 || _stricmp(argv[1], "-run") == 0)) {
        return ExecuteIpPurge();
    }

    // Coba mulai sebagai Service terlebih dahulu
    SERVICE_TABLE_ENTRYA ServiceTable[] = {
        { (LPSTR)SERVICE_NAME, (LPSERVICE_MAIN_FUNCTIONA)ServiceMain },
        { NULL, NULL }
    };

    if (!StartServiceCtrlDispatcherA(ServiceTable)) {
        return ExecuteIpPurge();
    }

    return 0;
}
