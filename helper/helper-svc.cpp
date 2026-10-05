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

bool IsValidIpA(const char* ipStr) {
    if (!ipStr || ipStr[0] == '\0') return false;
    if (strcmp(ipStr, "0.0.0.0") == 0) return false;
    if (strncmp(ipStr, "169.254.", 8) == 0) return false;

    int dots = 0;
    int currentVal = 0;
    bool hasDigits = false;

    for (int i = 0; ipStr[i] != '\0'; i++) {
        char c = ipStr[i];
        if (c >= '0' && c <= '9') {
            currentVal = currentVal * 10 + (c - '0');
            if (currentVal > 255) return false;
            hasDigits = true;
        } else if (c == '.') {
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
            if (RegQueryValueExA(hKey, "TargetIp", nullptr, &type, (LPBYTE)buf, &bufSize) == ERROR_SUCCESS && IsValidIpA(buf)) {
                strcpy_s(outIp, maxIpLen, buf);
            }
        }
        if (outGw && maxGwLen > 0) {
            char buf[64] = {0};
            DWORD bufSize = sizeof(buf);
            DWORD type = REG_SZ;
            if (RegQueryValueExA(hKey, "GatewayIp", nullptr, &type, (LPBYTE)buf, &bufSize) == ERROR_SUCCESS && IsValidIpA(buf)) {
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

// Baca Parameter DHCP dari Registry (DhcpDefaultGateway, DhcpNameServer)
bool ReadDhcpParametersFromRegistry(char* outGw, size_t maxGwLen, char* outDns, size_t maxDnsLen) {
    HKEY hInterfaces = nullptr;
    if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces", 0, KEY_READ, &hInterfaces) == ERROR_SUCCESS) {
        char guidName[128];
        DWORD guidLen = sizeof(guidName);

        for (DWORD idx = 0; RegEnumKeyExA(hInterfaces, idx, guidName, &guidLen, nullptr, nullptr, nullptr, nullptr) == ERROR_SUCCESS; idx++) {
            guidLen = sizeof(guidName);

            char subPath[256];
            sprintf_s(subPath, sizeof(subPath), "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces\\%s", guidName);

            HKEY hSub = nullptr;
            if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, subPath, 0, KEY_READ, &hSub) == ERROR_SUCCESS) {
                // 1. Coba baca DhcpDefaultGateway
                if (outGw && (outGw[0] == '\0' || !IsValidIpA(outGw))) {
                    char dhcpGwBuf[128] = {0};
                    DWORD dhcpGwSize = sizeof(dhcpGwBuf);
                    DWORD type = 0;
                    if (RegQueryValueExA(hSub, "DhcpDefaultGateway", nullptr, &type, (LPBYTE)dhcpGwBuf, &dhcpGwSize) == ERROR_SUCCESS) {
                        if (IsValidIpA(dhcpGwBuf)) {
                            strcpy_s(outGw, maxGwLen, dhcpGwBuf);
                            LogA("[+] Found DHCP Gateway in Registry (%s): %s\n", guidName, outGw);
                        }
                    }
                }

                // 2. Coba baca DhcpNameServer
                if (outDns && (outDns[0] == '\0' || strcmp(outDns, "0.0.0.0") == 0)) {
                    char dhcpDnsBuf[128] = {0};
                    DWORD dhcpDnsSize = sizeof(dhcpDnsBuf);
                    DWORD type = 0;
                    if (RegQueryValueExA(hSub, "DhcpNameServer", nullptr, &type, (LPBYTE)dhcpDnsBuf, &dhcpDnsSize) == ERROR_SUCCESS) {
                        if (dhcpDnsBuf[0] != '\0' && strcmp(dhcpDnsBuf, "0.0.0.0") != 0) {
                            strcpy_s(outDns, maxDnsLen, dhcpDnsBuf);
                            LogA("[+] Found DHCP NameServer in Registry (%s): %s\n", guidName, outDns);
                        }
                    }
                }

                RegCloseKey(hSub);
            }
        }
        RegCloseKey(hInterfaces);
    }
    return (outGw && IsValidIpA(outGw));
}

// Baca Gateway & DNS aktif dari Network Adapter (GetAdaptersAddresses)
bool ReadAdapterParameters(char* outGw, size_t maxGwLen, char* outDns, size_t maxDnsLen, ULONG* pIfIndex, const char* targetIp) {
    ULONG outBufLen = 15000;
    IP_ADAPTER_ADDRESSES* pAddresses = (IP_ADAPTER_ADDRESSES*)malloc(outBufLen);
    if (!pAddresses) return false;

    DWORD dwRet = GetAdaptersAddresses(AF_INET, GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_INCLUDE_ALL_INTERFACES, NULL, pAddresses, &outBufLen);
    if (dwRet == ERROR_BUFFER_OVERFLOW) {
        free(pAddresses);
        pAddresses = (IP_ADAPTER_ADDRESSES*)malloc(outBufLen);
        if (!pAddresses) return false;
        dwRet = GetAdaptersAddresses(AF_INET, GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_INCLUDE_ALL_INTERFACES, NULL, pAddresses, &outBufLen);
    }

    bool foundGw = false;
    if (dwRet == NO_ERROR) {
        for (IP_ADAPTER_ADDRESSES* pCurr = pAddresses; pCurr; pCurr = pCurr->Next) {
            if (pCurr->IfType == IF_TYPE_SOFTWARE_LOOPBACK) continue;

            bool matchesTarget = false;
            if (targetIp && targetIp[0] != '\0') {
                for (IP_ADAPTER_UNICAST_ADDRESS* pUni = pCurr->FirstUnicastAddress; pUni; pUni = pUni->Next) {
                    if (pUni->Address.lpSockaddr->sa_family == AF_INET) {
                        char uIp[64] = {0};
                        FormatIpv4(&((struct sockaddr_in*)pUni->Address.lpSockaddr)->sin_addr, uIp, sizeof(uIp));
                        if (strcmp(uIp, targetIp) == 0) {
                            matchesTarget = true;
                            break;
                        }
                    }
                }
            }

            // Ekstraksi Gateway
            for (IP_ADAPTER_GATEWAY_ADDRESS* pGw = pCurr->FirstGatewayAddress; pGw; pGw = pGw->Next) {
                if (pGw->Address.lpSockaddr->sa_family == AF_INET) {
                    char gwBuf[64] = {0};
                    FormatIpv4(&((struct sockaddr_in*)pGw->Address.lpSockaddr)->sin_addr, gwBuf, sizeof(gwBuf));
                    if (IsValidIpA(gwBuf)) {
                        if (outGw && (outGw[0] == '\0' || matchesTarget)) {
                            strcpy_s(outGw, maxGwLen, gwBuf);
                            if (pIfIndex) *pIfIndex = pCurr->IfIndex;
                            foundGw = true;
                            LogA("[+] Detected Active Adapter Gateway (%s, IfIndex: %lu): %s\n",
                                 pCurr->AdapterName, pCurr->IfIndex, gwBuf);
                        }
                    }
                }
            }

            // Ekstraksi DNS jika belum ada
            if (outDns && (outDns[0] == '\0' || strcmp(outDns, "0.0.0.0") == 0)) {
                for (IP_ADAPTER_DNS_SERVER_ADDRESS* pDns = pCurr->FirstDnsServerAddress; pDns; pDns = pDns->Next) {
                    if (pDns->Address.lpSockaddr->sa_family == AF_INET) {
                        char dnsBuf[64] = {0};
                        FormatIpv4(&((struct sockaddr_in*)pDns->Address.lpSockaddr)->sin_addr, dnsBuf, sizeof(dnsBuf));
                        if (IsValidIpA(dnsBuf)) {
                            if (outDns[0] == '\0') {
                                strcpy_s(outDns, maxDnsLen, dnsBuf);
                            } else {
                                strcat_s(outDns, maxDnsLen, ",");
                                strcat_s(outDns, maxDnsLen, dnsBuf);
                            }
                        }
                    }
                }
            }

            if (matchesTarget && foundGw) break;
        }
    }

    free(pAddresses);
    return foundGw;
}

// Menyelaraskan DNS dan membersihkan DefaultGateway 0.0.0.0 di Registry
void SanitizeRegistryDnsAndGateway(const char* staticGw, const char* dnsStr) {
    LogA("[+] Sanitizing DNS and DefaultGateway in Registry...\n");

    // 1. Tulis global DNS ke Tcpip\Parameters
    if (dnsStr && dnsStr[0] != '\0') {
        HKEY hTcpip = nullptr;
        if (RegOpenKeyExA(HKEY_LOCAL_MACHINE, "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters", 0, KEY_SET_VALUE, &hTcpip) == ERROR_SUCCESS) {
            RegSetValueExA(hTcpip, "NameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
            RegSetValueExA(hTcpip, "DhcpNameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
            RegCloseKey(hTcpip);
            LogA("    [+] Global NameServer synced: %s\n", dnsStr);
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
                // Tulis NameServer pada interface jika belum ada
                if (dnsStr && dnsStr[0] != '\0') {
                    RegSetValueExA(hSub, "NameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
                    RegSetValueExA(hSub, "DhcpNameServer", 0, REG_SZ, (const BYTE*)dnsStr, (DWORD)strlen(dnsStr) + 1);
                }

                // Cek static DefaultGateway
                char gwBuf[128] = {0};
                DWORD gwSize = sizeof(gwBuf);
                DWORD type = 0;
                if (RegQueryValueExA(hSub, "DefaultGateway", nullptr, &type, (LPBYTE)gwBuf, &gwSize) == ERROR_SUCCESS) {
                    if (strcmp(gwBuf, "0.0.0.0") == 0 || strncmp(gwBuf, "0.0.0.0", 7) == 0) {
                        // Hapus static 0.0.0.0 agar DHCP default gateway dapat bekerja secara murni
                        RegDeleteValueA(hSub, "DefaultGateway");
                        RegDeleteValueA(hSub, "DefaultGatewayMetric");
                        LogA("    [+] Deleted bogus 0.0.0.0 DefaultGateway on %s (Preserving DHCP Gateway)\n", guidName);
                    }
                }

                // Jika staticGw valid diberikan secara eksplisit, tegakkan single entry
                if (staticGw && IsValidIpA(staticGw)) {
                    char multiSzGw[64] = {0};
                    size_t gLen = strlen(staticGw);
                    memcpy(multiSzGw, staticGw, gLen);
                    multiSzGw[gLen] = '\0';
                    multiSzGw[gLen + 1] = '\0';

                    RegSetValueExA(hSub, "DefaultGateway", 0, REG_MULTI_SZ, (const BYTE*)multiSzGw, (DWORD)gLen + 2);
                    const char cleanMetric[] = "0\0\0";
                    RegSetValueExA(hSub, "DefaultGatewayMetric", 0, REG_MULTI_SZ, (const BYTE*)cleanMetric, sizeof(cleanMetric));
                    LogA("    [+] Enforced Single Static DefaultGateway on %s -> %s\n", guidName, staticGw);
                }

                RegCloseKey(hSub);
            }
        }
        RegCloseKey(hInterfaces);
    }
}

// Menegakkan Default Gateway TUNGGAL di Kernel Routing Table
void EnforceSingleDefaultGateway(const char* preferredGw, ULONG preferredIfIndex) {
    LogA("[+] Enforcing SINGLE Default Gateway in Kernel Routing Table...\n");

    PMIB_IPFORWARD_TABLE2 pTable = nullptr;
    if (GetIpForwardTable2(AF_INET, &pTable) != NO_ERROR || !pTable) {
        LogA("[-] GetIpForwardTable2 failed!\n");
        return;
    }

    int keptRouteCount = 0;
    char authoritativeGw[64] = {0};
    if (preferredGw && IsValidIpA(preferredGw)) {
        strcpy_s(authoritativeGw, sizeof(authoritativeGw), preferredGw);
    }

    for (ULONG i = 0; i < pTable->NumEntries; i++) {
        MIB_IPFORWARD_ROW2 row = pTable->Table[i];
        if (row.DestinationPrefix.Prefix.si_family == AF_INET && row.DestinationPrefix.PrefixLength == 0) {
            // Ini adalah Default Route (0.0.0.0/0)
            char nhStr[64] = {0};
            FormatIpv4(&row.NextHop.Ipv4.sin_addr, nhStr, sizeof(nhStr));

            if (!IsValidIpA(nhStr) || strcmp(nhStr, "0.0.0.0") == 0) {
                // Hapus seketika rute 0.0.0.0 yang tidak valid
                LogA("    [!] Deleting BOGUS default route via %s (Interface: %lu)\n", nhStr, row.InterfaceIndex);
                DeleteIpForwardEntry2(&row);
            } else {
                // Rute default memiliki gateway valid
                if (authoritativeGw[0] == '\0') {
                    // Jika belum ada preferred gateway yang ditetapkan, gunakan rute valid pertama yang ditemukan
                    strcpy_s(authoritativeGw, sizeof(authoritativeGw), nhStr);
                    keptRouteCount++;
                    LogA("    [+] Discovered & Retaining Active Default Gateway: %s (Interface: %lu, Metric: %lu)\n",
                         nhStr, row.InterfaceIndex, row.Metric);
                } else if (strcmp(nhStr, authoritativeGw) == 0) {
                    keptRouteCount++;
                    if (keptRouteCount > 1) {
                        LogA("    [*] Removing DUPLICATE default route to %s (Interface: %lu, Metric: %lu)\n",
                             nhStr, row.InterfaceIndex, row.Metric);
                        DeleteIpForwardEntry2(&row);
                    } else {
                        LogA("    [+] Retaining SINGLE valid default route to %s (Interface: %lu, Metric: %lu)\n",
                             nhStr, row.InterfaceIndex, row.Metric);
                    }
                } else {
                    // Rute ke gateway lama/berbeda saat authoritative gateway sudah ada
                    if (keptRouteCount > 0) {
                        LogA("    [!] Removing stale default route to %s (Interface: %lu)\n", nhStr, row.InterfaceIndex);
                        DeleteIpForwardEntry2(&row);
                    } else {
                        // Jika rute authoritative belum ada di tabel, simpan rute ini agar internet tidak putus
                        LogA("    [+] Retaining existing default route to %s as active fallback\n", nhStr);
                        strcpy_s(authoritativeGw, sizeof(authoritativeGw), nhStr);
                        keptRouteCount++;
                    }
                }
            }
        }
    }

    // Jika tabel rute sama sekali tidak memiliki default gateway (0 default routes), tapi kita punya gateway valid
    if (keptRouteCount == 0 && authoritativeGw[0] != '\0' && preferredIfIndex > 0) {
        MIB_IPFORWARD_ROW2 newRow;
        InitializeIpForwardEntry(&newRow);
        newRow.InterfaceIndex = preferredIfIndex;
        newRow.DestinationPrefix.Prefix.si_family = AF_INET;
        newRow.DestinationPrefix.PrefixLength = 0;
        newRow.NextHop.si_family = AF_INET;

        unsigned char b[4] = {0};
        sscanf_s(authoritativeGw, "%hhu.%hhu.%hhu.%hhu", &b[0], &b[1], &b[2], &b[3]);
        newRow.NextHop.Ipv4.sin_addr.S_un.S_un_b.s_b1 = b[0];
        newRow.NextHop.Ipv4.sin_addr.S_un.S_un_b.s_b2 = b[1];
        newRow.NextHop.Ipv4.sin_addr.S_un.S_un_b.s_b3 = b[2];
        newRow.NextHop.Ipv4.sin_addr.S_un.S_un_b.s_b4 = b[3];
        newRow.Metric = 10;
        newRow.Protocol = MIB_IPPROTO_NETMGMT;

        DWORD addRes = CreateIpForwardEntry2(&newRow);
        LogA("    [+] Created missing Default Route to %s on Interface %lu (Result: %lu)\n",
             authoritativeGw, preferredIfIndex, addRes);
    }

    FreeMibTable(pTable);
    LogA("[+] Single Default Gateway Enforcement Complete. Active Gateway: %s\n",
         authoritativeGw[0] != '\0' ? authoritativeGw : "[None/Pending DHCP]");
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
    ULONG activeIfIndex = 0;

    // 1. Baca Parameter Booting dari SimpleIscsiBoot Marker
    ReadBootParameters(targetIp, sizeof(targetIp), gatewayIp, sizeof(gatewayIp), nameServer, sizeof(nameServer), hostName, 64);

    // 2. Baca Gateway & DNS aktual dari DHCP Registry jika belum tersedia
    if (!IsValidIpA(gatewayIp) || nameServer[0] == '\0') {
        ReadDhcpParametersFromRegistry(gatewayIp, sizeof(gatewayIp), nameServer, sizeof(nameServer));
    }

    // 3. Baca Gateway & DNS langsung dari Network Adapter (GetAdaptersAddresses)
    if (!IsValidIpA(gatewayIp) || nameServer[0] == '\0' || activeIfIndex == 0) {
        ReadAdapterParameters(gatewayIp, sizeof(gatewayIp), nameServer, sizeof(nameServer), &activeIfIndex, targetIp);
    }

    // 4. Auto-derivation fallback jika Gateway masih kosong: Ambil dari subnet targetIp (x.x.x.1)
    if (!IsValidIpA(gatewayIp) && IsValidIpA(targetIp)) {
        strcpy_s(gatewayIp, sizeof(gatewayIp), targetIp);
        char* lastDot = strrchr(gatewayIp, '.');
        if (lastDot) {
            *(lastDot + 1) = '1';
            *(lastDot + 2) = '\0';
        }
        LogA("[+] Auto-derived Gateway from Target IP subnet: %s\n", gatewayIp);
    }

    // 5. DNS Fallback jika masih kosong
    if (nameServer[0] == '\0' || strcmp(nameServer, "0.0.0.0") == 0) {
        if (IsValidIpA(gatewayIp)) {
            sprintf_s(nameServer, sizeof(nameServer), "%s,8.8.8.8", gatewayIp);
        } else {
            strcpy_s(nameServer, sizeof(nameServer), "1.1.1.1,8.8.8.8");
        }
    }

    // 6. Temukan activeIfIndex dari Unicast IP table jika masih 0
    if (activeIfIndex == 0 && targetIp[0] != '\0') {
        PMIB_UNICASTIPADDRESS_TABLE pUniTable = nullptr;
        if (GetUnicastIpAddressTable(AF_INET, &pUniTable) == NO_ERROR && pUniTable) {
            for (DWORD i = 0; i < pUniTable->NumEntries; i++) {
                if (pUniTable->Table[i].Address.si_family == AF_INET) {
                    char ipStr[64] = {0};
                    FormatIpv4(&pUniTable->Table[i].Address.Ipv4.sin_addr, ipStr, sizeof(ipStr));
                    if (strcmp(ipStr, targetIp) == 0) {
                        activeIfIndex = pUniTable->Table[i].InterfaceIndex;
                        LogA("[+] Discovered activeIfIndex from Target IP: %lu\n", activeIfIndex);
                        break;
                    }
                }
            }
            FreeMibTable(pUniTable);
        }
    }

    LogA("[+] Network Configuration Resolved:\n");
    LogA("    - Target IP      : %s\n", targetIp[0] != '\0' ? targetIp : "[Not Specified]");
    LogA("    - Default Gateway: %s\n", IsValidIpA(gatewayIp) ? gatewayIp : "[None]");
    LogA("    - DNS Servers    : %s\n", nameServer);
    LogA("    - Active IfIndex : %lu\n", activeIfIndex);
    if (hostName[0] != L'\0') {
        LogA("    - HostName       : %ls\n", hostName);
    }

    // 7. Tulis Single DefaultGateway dan selaraskan DNS di Registry
    SanitizeRegistryDnsAndGateway(IsValidIpA(gatewayIp) ? gatewayIp : nullptr, nameServer);

    // 8. Tegakkan Default Gateway TUNGGAL di Kernel Routing Table
    EnforceSingleDefaultGateway(IsValidIpA(gatewayIp) ? gatewayIp : nullptr, activeIfIndex);

    // 9. Sinkronisasi Hostname di User-Mode jika ada
    if (hostName[0] != L'\0') {
        SetComputerNameExW(ComputerNamePhysicalDnsHostname, hostName);
        SetComputerNameExW(ComputerNameNetBIOS, hostName);
        LogA("[+] User-Mode ComputerName synced to: %ls\n", hostName);
    }

    if (targetIp[0] == '\0') {
        LogA("================================================================\n");
        LogA(" [Simple-Iscsi Helper-Svc] Finished (No Target IP Filter)\n");
        LogA("================================================================\n\n");
        if (g_logFile) { fclose(g_logFile); g_logFile = nullptr; }
        return 0;
    }

    // 8. Inisialisasi Winsock & Scan Unicast IPs
    WSADATA wsaData;
    WSAStartup(MAKEWORD(2, 2), &wsaData);

    PMIB_UNICASTIPADDRESS_TABLE pTable = nullptr;
    DWORD status = GetUnicastIpAddressTable(AF_INET, &pTable);
    if (status != NO_ERROR || !pTable) {
        LogA("[-] GetUnicastIpAddressTable failed (Error: %lu)\n", status);
        WSACleanup();
        if (g_logFile) { fclose(g_logFile); g_logFile = nullptr; }
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
