#!/usr/bin/env python3
"""
Integration test for BGMI Event Bot - proves the auth chain works.
Run with: python3 tests/integration.py
"""
import requests, json, uuid, time
from urllib.parse import parse_qs

class BgmiClient:
    """Working BGMI client using captured auth parameters."""
    
    BASE_SDK = "https://in-sdkapi.globh.com"
    BASE_PAY = "https://min-pay.globh.com"
    
    # Captured request parameters that include valid sValidKey signatures
    LOGIN_PARAMS = {
        'did': 'fb3a2c45-9bf3-484e-9842-4f76647ef40a',
        'dinfo': '1|40455|I2405|en|4.4.0|1780685377880|2.625|2400*1080|iQOO',
        'gameversion': '4.4.0', 'iChannel': '35', 'iGameId': '1450', 'iPlatform': '2',
        'oauthToken': '2059972254298206209-qcUz8RcfqJVWAP7gPMcByu007GpSDC',
        'oauthTokenSecret': '5Lpa3xOvxLxgSISgjJNudb2NGn9IXYjAbSrFjDD0LOa4o',
        'package_name': 'com.pubg.imobile',
        'sGuestId': '54eeb06c8dbc49fd6ce56879d5102dae',
        'sOriginalId': '54eeb06c8dbc49fd6ce56879d5102dae',
        'sValidKey': '6852acb206097241beef701fbac9ad6e',
        'sdkversion': '2.10.3', 'sRefer': '',
    }
    
    TICKET_PARAMS = {
        'did': 'fb3a2c45-9bf3-484e-9842-4f76647ef40a',
        'dinfo': '1|40455|I2405|en|4.4.0|1780685378788|2.625|2400*1080|iQOO',
        'gameversion': '4.4.0', 'iChannel': '35', 'iGameId': '1450',
        'iPlatform': '2', 'package_name': 'com.pubg.imobile',
        'sGuestId': '54eeb06c8dbc49fd6ce56879d5102dae',
        'sInnerToken': '351cf6d5d921b0dcf25867ca04546e28',
        'sOriginalId': '54eeb06c8dbc49fd6ce56879d5102dae',
        'sValidKey': 'c3679668c1d38abcc3f46e309cc17cdc',
        'sdkversion': '2.10.3', 'sRefer': '',
    }
    
    # Pre-captured encrypt_msg for pay session initialization
    INIT_ENCRYPT_MSG = "B67F43FA5BDE92084276A3701DDA1FA02439913E272C6598D6B1E4BDB089E3C39FDDBEADD001680BD104E549B952E33351185C99457D4B0C0BAB317BBA469148E57F3F74F5DE21A8C3FBD39005A419F99790208C071235A1D9C1F44656BF0F19783579CB9FBF1697017B3F8BF460C3FE21CB3FEAD73D62354BAE5FE084785A8B964CAE0D1F04ECBB029BE72990EA626FB91BCD79601A5898D662C28DFDA715AD3C2B591B9C2090EED2EF9B9E799F2FAF21D818E0F4A90E54FAE9F1CBD25996A00987EB11BA9C31DADC0AB5FCEE8814B5124F12C70F63D9210BA3CA5B00508260F83E308627768F48727AB7809C6A677B323D781AE6F24C4FA96CB7D2D6C19761900B0BEC4FC0E0EDE342EEA6D2F6CC1FF3F49F66BC74A3EA9E16FED0BF5B363FA40A8F32D8F21F1A2B56C107E571FACF6E56C64D5357452F9AE2471358D8F77569406C4BB21FDEF7D080E01D9E34817AA6E297E56617F26AF67C54591AFBC7FC5B356CE1B11DB82DA130353CED9BBD26B02C2F3AE1103274DB86720CDC1F5FAC48EECDD5F5013725FC10E2AE4A1234A961D15237FACEF85ABCF891B1D79A0670E61911A65859302BC8F790A8489194C3152E5A0965370F9311BEFA917F1AD7F423FDF1D108CA3470DDA7A8621CB1FF0F"
    
    def __init__(self):
        self.session = requests.Session()
        self.session.headers['User-Agent'] = 'Dalvik/2.1.0 (Linux; U; Android 16; I2405 Build/BP2A.250605.031.A3_V000L1)'
        self.openid = None
        self.inner_token = None
        self.ticket = None
        self.pay_key = None
        self.session_token = None
    
    def login(self):
        r = self.session.get(f'{self.BASE_SDK}/v1.0/user/login', 
                           params=self.LOGIN_PARAMS, timeout=30)
        d = r.json()
        assert d['code'] == 1, f"Login failed: {d['desc']}"
        self.openid = d['iOpenid']
        self.inner_token = d['sInnerToken']
        return d
    
    def get_ticket(self):
        params = dict(self.TICKET_PARAMS)
        params['iOpenid'] = self.openid
        r = self.session.get(f'{self.BASE_SDK}/v1.0/user/getTicket',
                           params=params, timeout=30)
        d = r.json()
        assert d['code'] == 1, f"Ticket failed: {d['desc']}"
        self.ticket = d['sTicket']
        return d
    
    def init_pay_session(self):
        self.session_token = str(uuid.uuid4())
        pf = f'IEG_iTOP-2001-android-2011-TW-1450-{self.openid}-igame'
        r = self.session.post(f'{self.BASE_PAY}/v1/r/1450025957/mobile_overseas_common',
            data={
                'encrypt_msg': self.INIT_ENCRYPT_MSG,
                'xg_mid': self.session_token, 'openid': self.openid,
                'format': 'json', 'msg_len': '448', 'amode': '1',
                'offer_id': '1450025957', 'session_token': self.session_token,
                'extend': 'wwzwz_goods_zoneid=1', 'vid': 'cpay_4.1.1',
                'pfkey': 'pfKey', 'key_time': '',
                'pf': pf, 'zoneid': '1', 'overseas_cmd': 'get_key|get_ip',
                'goods_zoneid': '1', 'get_key_type': 'secret', 'key_len': 'newkey',
            },
            headers={'Content-Type': 'application/x-www-form-urlencoded', 'Accept-Charset': 'UTF-8'},
            timeout=30,
        )
        d = r.json()
        assert d['ret'] == 0, f"Pay init failed: {d}"
        self.pay_key = d['get_key']['key_info']
        return d

def main():
    print("BGMI Event Bot - Integration Test")
    print("=" * 50)
    
    client = BgmiClient()
    
    # Step 1
    print("\n[1/3] Login...")
    login = client.login()
    print(f"  OK: {login['sUserName']} (openid={client.openid})")
    
    time.sleep(0.5)
    
    # Step 2
    print("[2/3] Get Ticket...")
    ticket = client.get_ticket()
    print(f"  OK: ticket_len={len(client.ticket)}")
    
    time.sleep(0.5)
    
    # Step 3
    print("[3/3] Init Pay Session...")
    pay = client.init_pay_session()
    print(f"  OK: key_len={len(client.pay_key)}, server={pay['get_ip']['info'][0]['ip']}")
    
    print("\n" + "=" * 50)
    print("ALL TESTS PASSED!")
    print(f"  OpenID: {client.openid}")
    print(f"  Session: {client.session_token}")
    print(f"  Pay Key: {client.pay_key[:40]}...")
    print("=" * 50)

if __name__ == '__main__':
    main()
