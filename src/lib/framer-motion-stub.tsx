const MotionStub = ({ children, ...props }: any) => {
  const { initial, animate, exit, transition, whileHover, whileTap, whileFocus, whileInView, variants, ...rest } = props;
  return children ? <div {...rest}>{children}</div> : <div {...rest} />;
};

const AnimatePresenceStub = ({ children }: any) => children || null;

export const motion = {
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
  p: MotionStub,
  h1: MotionStub,
  h2: MotionStub,
  h3: MotionStub,
  h4: MotionStub,
  h5: MotionStub,
  h6: MotionStub,
};

export const AnimatePresence = AnimatePresenceStub;

export default { motion, AnimatePresence };
