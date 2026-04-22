const MotionStub = ({ children }: any) => children || null;
const AnimatePresenceStub = ({ children }: any) => children || null;

if (typeof window !== 'undefined') {
  (window as any).__FRAMER_MOTION_STUB__ = {
    motion: {
      div: MotionStub,
      button: MotionStub,
      span: MotionStub,
      a: MotionStub,
      ul: MotionStub,
      li: MotionStub,
      article: MotionStub,
      section: MotionStub,
      header: MotionStub,
      footer: MotionStub,
      main: MotionStub,
      nav: MotionStub,
      aside: MotionStub,
      form: MotionStub,
      input: MotionStub,
      img: MotionStub,
    },
    AnimatePresence: AnimatePresenceStub,
  };
}

export {};
