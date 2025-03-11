// import { binary_to_base58 as binaryToBase58, base58_to_binary as base58ToBinary } from "base58-js";
import * as buffer from 'buffer';
import { toBigIntBE } from "bigint-buffer";
import { Account, base58Decode, ECDSAKeyPair } from "@affidaty/t2-lib-core";

// export const synPayComponentExist = (): boolean =>
// !!document.querySelector(`script[src^="${import.meta.env.VITE_SYNPAY_CDN}"`)

/**
 * Auth in dynamic import based on env
 */
// export const addSynPayCdnImports = () => {
//   if (synPayComponentExist()){
//     return
//   }
//   const synEsm = document.createElement('script')
//   synEsm.setAttribute('type', `module`)
//   synEsm.setAttribute('data-apiKey', import.meta.env.VITE_MERCHANT_APIKEY)
//   synEsm.setAttribute('data-env', import.meta.env.VITE_SYNPAY_ENVIRONMENT)
//   synEsm.setAttribute('data-appid', document.location.host)
//   synEsm.setAttribute('src', `${import.meta.env.VITE_SYNPAY_CDN}/affidaty-pay-component.esm.js`)
//   document.head.appendChild(synEsm)
// }

export const authInComponentExist = (): boolean =>
!!document.querySelector(`script[src^="${import.meta.env.VITE_AUTHIN_CDN}"`)

/**
 * Auth in dynamic import based on env
 */
export const addAuthinCdnImports = () => {
  if (authInComponentExist()){
    return
  }
  const authinEsm = document.createElement('script')
  authinEsm.setAttribute('type', `module`)
  authinEsm.setAttribute('data-apikey', import.meta.env.VITE_AUTHIN_APIKEY || '')
  authinEsm.setAttribute('data-env', import.meta.env.VITE_AUTHIN_ENVIRONMENT)
  authinEsm.setAttribute('data-appname', import.meta.env.VITE_AUTHIN_APPLICATION)
  // authinEsm.setAttribute('data-appid', 'io.affidaty.gogo')
  authinEsm.setAttribute('data-fields', 'name,surname,phone')
  authinEsm.setAttribute('src', `${import.meta.env.VITE_AUTHIN_CDN}/affidaty-auth-in.esm.js`)
  document.head.appendChild(authinEsm)
}

export const getUuidString = (lenght: number = 32) => {
  const length = 32;
  let result = '';
  const characters = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  const charactersLength = characters.length;
  let counter = 0;
  while (counter < length) {
      result += characters.charAt(Math.floor(Math.random() * charactersLength));
      counter += 1;
  }
  return result;
}

export const convertvalueToBc = (value: number, decimal: number) => Number((value / Math.pow(10, decimal)).toFixed(decimal));


/**
* Base58 to ArrayBuffer transformation
*
* @param {String} base58
* @returns {ArrayBuffer} the ArrayBuffer string encoding
*/
// export function base58ToArrayBuffer(base58: any) {
//   return base58ToBinary(base58).buffer;
// }

/**
* From ArrayBuffer to Buffer
*
* @param arrayBuffer
* @return {Buffer}
*/
export function arrayBufferToBuffer(arrayBuffer: any) {
  return buffer.Buffer.from(arrayBuffer);
}

/**
* From Buffer to ArrayBuffer
*
* @param buffer buffer to convert
* @return {arrayBuffer}
*/
export function bufferToArrayBuffer(buffer: any) {
  const arrayBuffer = new ArrayBuffer(buffer.length);
  const arrayBufferView = new Uint8Array(arrayBuffer);
  for (let i = 0; i < buffer.length; ++i) {
      arrayBufferView[i] = buffer[i];
  }
  return arrayBuffer;
}


/**
* Base58 to Buffer transformation
*
* @param {String} base58
* @returns {Buffer} the Buffer string encoding
*/
// export function base58ToBuffer(base58: any) {
//   const buffer = base58ToBinary(base58).buffer;
//   return arrayBufferToBuffer(buffer);
// }

/**
   * ArrayBuffer to base58 transformation
   *
   * @param {ArrayBuffer} buffer
   * @returns {String} the base58 buffer encoding
   */
// export function bufferToBase58(buffer: any) {
//   const bytes = new Uint8Array(buffer);
//   return binaryToBase58(bytes);
// }

/**
* Computes polling station index from arbitrary data
* @param idV idV of random wallet generated (account id)
* @param modulo total number of indexes
* @param saltBuffer
*/
export function indexFromData(idV = Buffer.from([0]), modulo = 10, salt: any = null) {
  return new Promise((resolve, reject) => {
      let buff = Buffer.from(idV);
      if (salt) {
          buff = Buffer.concat([buff, Buffer.from(salt)]);
      }
      window.crypto.subtle.digest("SHA-384", buff).then((digest: any) => { //Crypto.createHash('sha384').update(buff).digest();
          digest = arrayBufferToBuffer(digest);
          const idx = Number(toBigIntBE(digest) % BigInt(modulo));
          resolve(idx);
      }, err => { reject(err) });

  })
}

/**
* Converts buffer to a base64url string
* @param buffer{Buffer} input buffer
* @returns {String} base64url string
*/
export function bufferToBase64url(buffer: any) {
  return buffer.toString("base64").replace(/\+/g, '-').replace(/\//g, '_').replace(/=/g, '');
}

/**
* Converts buffer to a base64url string
* @param b64urlString{String} Base64url string to convert
* @returns {Buffer} buffer
*/
export function base64urlToBuffer(b64urlString: string) {
  return Buffer.from(b64urlString.replace(/-/g, '+').replace(/_/g, '/'), "base64");
}

/**
* Given an array of ArrayBuffer objects, this function concatenates them into a single ArrayBuffer
* @param arrayOfArrayBuffers{Array<ArrayBuffer>} array of ArrayBuffers
* @returns {ArrayBuffer} resulting ArrayBuffer
*/
export function concatArrayBuffers(arrayOfArrayBuffers: any) {
  let totalByteLength = 0;
  const offsets = [0];
  for (let i = 0; i < arrayOfArrayBuffers.length; i++) {
      totalByteLength += arrayOfArrayBuffers[i].byteLength;
      offsets.push(totalByteLength);
  }
  offsets.pop();
  const result = new ArrayBuffer(totalByteLength);
  const resultView = new Uint8Array(result);
  for (let i = 0; i < arrayOfArrayBuffers.length; i++) {
      resultView.set(new Uint8Array(arrayOfArrayBuffers[i]), offsets[i]);
  }
  return result;
}

export function clearStorage() {
  localStorage.removeItem('user_account')
  localStorage.removeItem('userInformations')
  localStorage.removeItem('account')
  localStorage.removeItem('user_information')
  localStorage.removeItem('trinci_token')
}

export const checkPermissions = (searchValue: string, searchSource: string[], strict = false): boolean => {
  return strict? searchSource.includes(searchValue) : searchSource.findIndex(p => p.includes(searchValue)) >= 0
}

export async function initAccount(publicKey: string): Promise<Account>{
  const account = new Account();
  const keyPair = new ECDSAKeyPair();
  return keyPair.publicKey.importBin(base58Decode(publicKey)).then(() => {
    return account.setKeyPair(keyPair);
  }).then(() => {
      localStorage.setItem('account', JSON.stringify(account));
      // console.log("[DEBUG] init account", account)
      return Promise.resolve(account)
  })
  .catch(() => Promise.reject())
}

export const isEmptyOrNull = (object: any) => {
  return !object || Object.keys(object).length === 0;
};

interface GenericFunctionType {
  <T>(time: number, target: T[], source: T[]): Promise<boolean>
}
export const lazyPush: GenericFunctionType = async (time, target, source) => {
  return new Promise(async (mainResolve) => {
    let nextTime = 0 - time;
    const lazyPromise = source.map((item) => {
      return () => {
        return new Promise((resolve) => {
          setTimeout(
            () => {
              target.push(item)
              resolve(true)
            },
            (nextTime += time)
          )
        })
      }
    })
    for await (const f of lazyPromise) {
      await f()
    }
    mainResolve(true)
  })
}


export const chop = (text: string, start: number, end: number , separator:string) => {
  return text.slice(0, start) + separator + text.slice(-1 * end);
}
