const U64_MASK = 0xffff_ffff_ffff_ffffn
// 64-bit FNV-1a constants from RFC 9923. The update is:
// hash = (hash ^ byte) * FNV64_PRIME mod 2^64.
const FNV64_OFFSET = 0xcbf2_9ce4_8422_2325n
const FNV64_PRIME = 0x0000_0100_0000_01b3n

// The hash algorithm is FNV-1a, but the typed value conversion is customized
export class U64Hasher {
  private hash = FNV64_OFFSET
  private readonly floatBuffer = new ArrayBuffer(8)
  private readonly floatView = new DataView(this.floatBuffer)

  reset() {
    this.hash = FNV64_OFFSET
  }

  writeByte(value: number) {
    this.hash ^= BigInt(value & 0xff)
    this.hash = (this.hash * FNV64_PRIME) & U64_MASK
  }

  writeBool(value: boolean) {
    this.writeByte(value ? 1 : 0)
  }

  writeUint32(value: number) {
    const unsigned = value >>> 0
    this.writeByte(unsigned)
    this.writeByte(unsigned >>> 8)
    this.writeByte(unsigned >>> 16)
    this.writeByte(unsigned >>> 24)
  }

  writeBigUint64(value: bigint) {
    let remaining = value & U64_MASK
    for (let i = 0; i < 8; i++) {
      this.writeByte(Number(remaining & 0xffn))
      remaining >>= 8n
    }
  }

  writeFloat64(value: number) {
    this.floatView.setFloat64(0, value, true)
    for (let i = 0; i < 8; i++) {
      this.writeByte(this.floatView.getUint8(i))
    }
  }

  writeString(value: string) {
    this.writeUint32(value.length)
    for (let i = 0; i < value.length; i++) {
      const char = value.charCodeAt(i)
      this.writeByte(char)
      this.writeByte(char >>> 8)
    }
  }

  writeUnknown(value: unknown) {
    switch (typeof value) {
      case 'bigint':
        this.writeByte(1)
        this.writeBigUint64(value)
        break
      case 'boolean':
        this.writeByte(2)
        this.writeBool(value)
        break
      case 'number':
        this.writeByte(3)
        this.writeFloat64(value)
        break
      case 'string':
        this.writeByte(4)
        this.writeString(value)
        break
      case 'undefined':
        this.writeByte(5)
        break
      default:
        if (value === null) {
          this.writeByte(6)
        } else {
          this.writeByte(7)
          this.writeString(String(value))
        }
    }
  }

  finish() {
    const hash = this.hash
    this.reset()
    return hash
  }
}
